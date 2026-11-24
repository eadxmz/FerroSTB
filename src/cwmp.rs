use anyhow::Result;
use flate2::read::GzDecoder;
use openssl::pkey::PKey;
use openssl::ssl::{Ssl, SslAcceptor, SslMethod, SslVerifyMode};
use openssl::x509::X509;
use std::collections::{HashSet, VecDeque};
use std::io::Read;
use std::net::IpAddr;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;
use tokio::sync::watch;
use tokio_openssl::SslStream;

const PRECOMPUTED_NETCHECK: &str = "UzLgdFHvJkBfRQiHnNQit+55dub3o1PbxIhuNZ7oJc8=";

static SESSION_COMPLETE: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StbFamily {
    Jhs,
    Jms,
}

#[derive(Clone)]
struct SoapDialect {
    env: String,
    enc: String,
    cwmp_ns: String,
    root: String,
}

impl Default for SoapDialect {
    fn default() -> Self {
        Self {
            env: "soap-env".to_string(),
            enc: "soap-enc".to_string(),
            cwmp_ns: "urn:dslforum-org:cwmp-1-0".to_string(),
            root: "Device".to_string(),
        }
    }
}

#[derive(Clone)]
enum RpcAction {
    GetParamNames { path: String, next_level: bool },
    SetSingleParam { name: String, xsd_type: String, value: String },
}

struct EngineState {
    family: StbFamily,
    dialect: SoapDialect,
    queue: VecDeque<RpcAction>,
    discovered_vendor_branches: Vec<String>,
    attempted_sets: HashSet<(String, String)>,
    any_set_succeeded: bool,
    rpc_counter: usize,
}

impl Default for EngineState {
    fn default() -> Self {
        Self {
            family: StbFamily::Jhs,
            dialect: SoapDialect::default(),
            queue: VecDeque::new(),
            discovered_vendor_branches: Vec::new(),
            attempted_sets: HashSet::new(),
            any_set_succeeded: false,
            rpc_counter: 0,
        }
    }
}

fn engine_state() -> &'static Mutex<EngineState> {
    static STATE: OnceLock<Mutex<EngineState>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(EngineState::default()))
}

pub async fn run_cwmp_servers(
    family: StbFamily,
    cert_pem: Vec<u8>,
    cert_label: &str,
    stb_ip_tx: watch::Sender<Option<IpAddr>>,
    stb_done_tx: watch::Sender<Option<IpAddr>>,
) -> Result<()> {
    if let Ok(mut st) = engine_state().lock() {
        st.family = family;
    }

    let certs = X509::stack_from_pem(&cert_pem)?;
    let pkey = PKey::private_key_from_pem(&cert_pem)?;

    let mut builder = SslAcceptor::mozilla_intermediate(SslMethod::tls_server())?;
    builder.set_min_proto_version(None)?;
    builder.set_cipher_list("DEFAULT:@SECLEVEL=0")?;
    builder.set_verify(SslVerifyMode::NONE);

    if let Some((leaf, chain)) = certs.split_first() {
        builder.set_certificate(leaf)?;
        for cert in chain {
            builder.add_extra_chain_cert(cert.to_owned())?;
        }
    }
    builder.set_private_key(&pkey)?;
    let acceptor = Arc::new(builder.build());

    for port in [8443u16, 9443, 7443, 443] {
        if let Ok(listener) = TcpListener::bind(("0.0.0.0", port)).await {
            println!("[CWMP-TLS] Listening on 0.0.0.0:{} ({})", port, cert_label);
            let acc = acceptor.clone();
            let ip_tx = stb_ip_tx.clone();
            let done_tx = stb_done_tx.clone();
            tokio::spawn(async move {
                loop {
                    if let Ok((tcp, peer)) = listener.accept().await {
                        let acc = acc.clone();
                        let ip_tx = ip_tx.clone();
                        let done_tx = done_tx.clone();
                        tokio::spawn(async move {
                            if !peer.ip().is_loopback() {
                                let _ = ip_tx.send_if_modified(|curr| {
                                    if curr.is_none() {
                                        *curr = Some(peer.ip());
                                        true
                                    } else {
                                        false
                                    }
                                });
                            }
                            if let Ok(ssl) = Ssl::new(acc.context()) {
                                if let Ok(mut tls_stream) = SslStream::new(ssl, tcp) {
                                    match Pin::new(&mut tls_stream).accept().await {
                                        Ok(()) => {
                                            handle_http_session(tls_stream, peer.ip(), port, done_tx).await;
                                        }
                                        Err(e) => {
                                            if port == 8443 || port == 9443 {
                                                eprintln!(
                                                    "[CWMP:{}] TLS handshake rejected by {}: {}",
                                                    port,
                                                    peer.ip(),
                                                    e
                                                );
                                                eprintln!(
                                                    "           -> Tip: Ensure you selected the right STB family (JHS needs eMudhra forged_chain.pem; JMS needs acs_chain.pem)."
                                                );
                                            }
                                        }
                                    }
                                }
                            }
                        });
                    }
                }
            });
        }
    }

    Ok(())
}

async fn handle_http_session(
    stream: SslStream<tokio::net::TcpStream>,
    peer_ip: IpAddr,
    port: u16,
    stb_done_tx: watch::Sender<Option<IpAddr>>,
) {
    let (read_half, mut write_half) = tokio::io::split(stream);
    let mut reader = BufReader::new(read_half);

    loop {
        let mut req_line = String::new();
        match reader.read_line(&mut req_line).await {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        let req_line_trimmed = req_line.trim_end().to_string();
        if req_line_trimmed.is_empty() {
            continue;
        }

        let mut content_len: usize = 0;
        let mut is_chunked = false;
        let mut is_gzip = false;

        loop {
            let mut hdr = String::new();
            if reader.read_line(&mut hdr).await.unwrap_or(0) == 0 {
                break;
            }
            if hdr == "\r\n" || hdr == "\n" {
                break;
            }
            if let Some((k, v)) = hdr.split_once(':') {
                let k_low = k.trim().to_ascii_lowercase();
                let v_low = v.trim().to_ascii_lowercase();
                if k_low == "content-length" {
                    content_len = v_low.parse().unwrap_or(0);
                } else if k_low == "transfer-encoding" && v_low.contains("chunked") {
                    is_chunked = true;
                } else if k_low == "content-encoding" && v_low.contains("gzip") {
                    is_gzip = true;
                }
            }
        }

        let mut raw_body = Vec::new();
        if is_chunked {
            loop {
                let mut size_line = String::new();
                if reader.read_line(&mut size_line).await.unwrap_or(0) == 0 {
                    break;
                }
                let size_hex = size_line.trim().split(';').next().unwrap_or("0");
                let chunk_size = usize::from_str_radix(size_hex, 16).unwrap_or(0);
                if chunk_size == 0 {
                    let mut trail = String::new();
                    let _ = reader.read_line(&mut trail).await;
                    break;
                }
                let mut chunk = vec![0u8; chunk_size];
                if reader.read_exact(&mut chunk).await.is_err() {
                    break;
                }
                raw_body.extend_from_slice(&chunk);
                let mut crlf = [0u8; 2];
                let _ = reader.read_exact(&mut crlf).await;
            }
        } else if content_len > 0 {
            raw_body.resize(content_len, 0);
            if reader.read_exact(&mut raw_body).await.is_err() {
                break;
            }
        }

        let body_str = if is_gzip && !raw_body.is_empty() {
            let mut d = GzDecoder::new(&raw_body[..]);
            let mut s = String::new();
            let _ = d.read_to_string(&mut s);
            s
        } else {
            String::from_utf8_lossy(&raw_body).to_string()
        };

        if req_line_trimmed.contains("network/check") {
            let json = format!("{{\"Network\":\"{}\"}}", PRECOMPUTED_NETCHECK);
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: keep-alive\r\n\r\n{}",
                json.len(),
                json
            );
            let _ = write_half.write_all(resp.as_bytes()).await;
            let _ = write_half.flush().await;
            println!("[CWMP:{}] {} <<< 200 OK netcheck-spoof (FTTH)", port, peer_ip);
            continue;
        }

        if req_line_trimmed.starts_with("GET ") {
            let resp = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}";
            let _ = write_half.write_all(resp.as_bytes()).await;
            let _ = write_half.flush().await;
            break;
        }

        if body_str.contains(":Inform") || body_str.contains("<Inform") {
            let cwmp_id = extract_cwmp_id(&body_str);
            let dialect = detect_dialect(&body_str);

            if !SESSION_COMPLETE.load(Ordering::SeqCst) {
                log_inform_details(&body_str, port);
                if let Ok(mut st) = engine_state().lock() {
                    st.dialect = dialect.clone();
                    st.rpc_counter = 0;
                    if st.queue.is_empty() {
                        let fam = st.family;
                        st.queue.push_back(RpcAction::GetParamNames {
                            path: format!("{}.", dialect.root),
                            next_level: true,
                        });
                        queue_isolated_fallbacks(&mut st.queue, &dialect.root, fam);
                    }
                }
            }

            let soap = inform_response_xml(&dialect, cwmp_id);
            let resp = format!(
                "HTTP/1.1 200 OK\r\nServer: GenieACS/1.2.9\r\nSOAPServer: GenieACS/1.2.9\r\nContent-Type: text/xml; charset=\"utf-8\"\r\nSet-Cookie: session=1337beef; Path=/\r\nContent-Length: {}\r\nConnection: keep-alive\r\n\r\n{}",
                soap.len(),
                soap
            );
            let _ = write_half.write_all(resp.as_bytes()).await;
            let _ = write_half.flush().await;
            println!(
                "[CWMP:{}] {} <<< 200 OK InformResponse (ID={}, root={})",
                port, peer_ip, cwmp_id, dialect.root
            );
            continue;
        }

        if SESSION_COMPLETE.load(Ordering::SeqCst) {
            send_204(&mut write_half).await;
            break;
        }

        if !body_str.trim().is_empty() {
            process_stb_response(&body_str, port);
        }

        let next_xml = {
            if let Ok(mut st) = engine_state().lock() {
                let mut chosen = None;
                while let Some(action) = st.queue.pop_front() {
                    match action {
                        RpcAction::GetParamNames { path, next_level } => {
                            st.rpc_counter += 1;
                            let id = st.rpc_counter.to_string();
                            println!(
                                "[CWMP:{}] {} <<< RPC #{}: GetParameterNames (path='{}', nextLevel={})",
                                port, peer_ip, id, path, next_level
                            );
                            chosen = Some(get_param_names_xml(&st.dialect, &id, &path, next_level));
                            break;
                        }
                        RpcAction::SetSingleParam { name, xsd_type, value } => {
                            if !st.attempted_sets.insert((name.clone(), value.clone())) {
                                continue;
                            }
                            st.rpc_counter += 1;
                            let id = st.rpc_counter.to_string();
                            println!(
                                "[CWMP:{}] {} <<< RPC #{}: SetParameterValues ({} = {})",
                                port, peer_ip, id, name, value
                            );
                            chosen = Some(set_single_param_xml(&st.dialect, &id, &name, &xsd_type, &value));
                            break;
                        }
                    }
                }
                chosen
            } else {
                None
            }
        };

        if let Some(soap) = next_xml {
            send_soap(&mut write_half, &soap).await;
        } else {
            let any_ok = engine_state()
                .lock()
                .map(|st| st.any_set_succeeded)
                .unwrap_or(false);
            if any_ok {
                SESSION_COMPLETE.store(true, Ordering::SeqCst);
            }
            send_204(&mut write_half).await;
            println!(
                "[CWMP:{}] {} <<< 204 No Content (CWMP Sequence Complete, set_ok={})",
                port, peer_ip, any_ok
            );
            let _ = stb_done_tx.send(Some(peer_ip));
            break;
        }
    }
}

fn log_inform_details(xml: &str, port: u16) {
    let mut info_items = Vec::new();
    for (name, val) in extract_param_values(xml) {
        if name.contains("SoftwareVersion")
            || name.contains("HardwareVersion")
            || name.contains("ModelName")
            || name.contains("ProductClass")
            || name.contains("ProvisioningCode")
        {
            info_items.push(format!("{}={}", name, val));
        }
    }
    if !info_items.is_empty() {
        println!("[CWMP:{}] STB Inform Details: {}", port, info_items.join(" | "));
    }
}

fn process_stb_response(xml: &str, port: u16) {
    if xml.contains("SetParameterValuesResponse") {
        let status = extract_tag_value(xml, "Status").unwrap_or("0");
        println!(
            "[CWMP:{}] [SUCCESS] STB accepted parameter update! (SetParameterValuesResponse Status={})",
            port, status
        );
        if let Ok(mut st) = engine_state().lock() {
            st.any_set_succeeded = true;
        }
    } else if xml.contains("GetParameterNamesResponse") {
        let params = extract_param_info_list(xml);
        println!(
            "[CWMP:{}] [DISCOVERY] STB returned {} parameter paths:",
            port,
            params.len()
        );
        if let Ok(mut st) = engine_state().lock() {
            let mut new_discoveries = Vec::new();
            let mut date_sets = Vec::new();
            let mut usb_sets = Vec::new();
            let mut fota_sets = Vec::new();

            for (name, writable) in params {
                println!("   -> {} (writable={})", name, writable);
                let low = name.to_ascii_lowercase();

                if name.ends_with('.') {
                    if (low.contains("jiostb")
                        || low.contains("jstb")
                        || low.contains("jio")
                        || low.contains("usb")
                        || low.contains("fota")
                        || low.contains("stb")
                        || low.contains("debug")
                        || low.contains("x_")
                        || low.contains("deviceinfo")
                        || low.contains("management"))
                        && !st.discovered_vendor_branches.contains(&name)
                    {
                        st.discovered_vendor_branches.push(name.clone());
                        new_discoveries.push(RpcAction::GetParamNames {
                            path: name.clone(),
                            next_level: false,
                        });
                    }
                } else if low.contains("debug") && low.contains("date") {
                    date_sets.push(RpcAction::SetSingleParam {
                        name: name.clone(),
                        xsd_type: "xsd:string".to_string(),
                        value: "2037-01-01_00:00:00".to_string(),
                    });
                    date_sets.push(RpcAction::SetSingleParam {
                        name: name.clone(),
                        xsd_type: "xsd:string".to_string(),
                        value: "31-12-2037 23:59:59".to_string(),
                    });
                    date_sets.push(RpcAction::SetSingleParam {
                        name: name.clone(),
                        xsd_type: "xsd:string".to_string(),
                        value: "2037-01-01T00:00:00.000Z".to_string(),
                    });
                } else if low.contains("usbdebugging") || (low.contains("adb") && writable) {
                    usb_sets.push(RpcAction::SetSingleParam {
                        name: name.clone(),
                        xsd_type: "xsd:boolean".to_string(),
                        value: "true".to_string(),
                    });
                    usb_sets.push(RpcAction::SetSingleParam {
                        name: name.clone(),
                        xsd_type: "xsd:boolean".to_string(),
                        value: "1".to_string(),
                    });
                } else if low.contains("fota") && low.contains("disable") {
                    fota_sets.push(RpcAction::SetSingleParam {
                        name: name.clone(),
                        xsd_type: "xsd:boolean".to_string(),
                        value: "true".to_string(),
                    });
                }
            }

            // Pop order: new_discoveries -> date_sets -> usb_sets -> fota_sets
            for item in fota_sets.into_iter().rev() {
                st.queue.push_front(item);
            }
            for item in usb_sets.into_iter().rev() {
                st.queue.push_front(item);
            }
            for item in date_sets.into_iter().rev() {
                st.queue.push_front(item);
            }
            for item in new_discoveries.into_iter().rev() {
                st.queue.push_front(item);
            }
        }
    } else if xml.contains("Fault") {
        let code = extract_tag_value(xml, "FaultCode").unwrap_or("unknown");
        let msg = extract_tag_value(xml, "FaultString").unwrap_or("unknown");
        if let Some(p_fault) = xml.find("SetParameterValuesFault>") {
            let sub = &xml[p_fault..];
            let pname = extract_tag_value(sub, "ParameterName").unwrap_or("?");
            let pcode = extract_tag_value(sub, "FaultCode").unwrap_or(code);
            let pmsg = extract_tag_value(sub, "FaultString").unwrap_or(msg);
            println!(
                "[CWMP:{}] STB Fault {} ({}) on '{}' [inner {}: {}] - advancing",
                port, code, msg, pname, pcode, pmsg
            );
        } else {
            println!(
                "[CWMP:{}] STB replied with Fault {} ({}) - advancing to next candidate",
                port, code, msg
            );
        }
    }
}

fn push_vendor_branch(q: &mut VecDeque<RpcAction>, root: &str, branch: &str) {
    q.push_back(RpcAction::GetParamNames {
        path: format!("{}.{}.", root, branch),
        next_level: false,
    });

    let candidates = [
        (format!("{}.{}.USB.DebugValidityDate", root, branch), "xsd:string", "2037-01-01_00:00:00"),
        (format!("{}.{}.USB.DebugValidityDate", root, branch), "xsd:string", "31-12-2037 23:59:59"),
        (format!("{}.{}.USB.DebugValidityDate", root, branch), "xsd:string", "2037-01-01T00:00:00.000Z"),
        (format!("{}.{}.USB.USBDebugging", root, branch), "xsd:boolean", "true"),
        (format!("{}.{}.USB.USBDebugging", root, branch), "xsd:boolean", "1"),
        (format!("{}.{}.FOTA.Disable", root, branch), "xsd:boolean", "true"),
    ];

    for (name, xsd_type, val) in candidates {
        q.push_back(RpcAction::SetSingleParam {
            name,
            xsd_type: xsd_type.to_string(),
            value: val.to_string(),
        });
    }
}

fn queue_isolated_fallbacks(q: &mut VecDeque<RpcAction>, root: &str, family: StbFamily) {
    match family {
        StbFamily::Jhs => {
            // JHSB200 / JHSC200 / JHSD200 use Device.JioSTB.* primarily
            push_vendor_branch(q, root, "JioSTB");
            push_vendor_branch(q, root, "JSTB");
        }
        StbFamily::Jms => {
            // JMSC200A / JMSB200A / JMSD200A use Device.JSTB.* primarily
            push_vendor_branch(q, root, "JSTB");
            push_vendor_branch(q, root, "JioSTB");
        }
    }
}

async fn send_soap(write_half: &mut tokio::io::WriteHalf<SslStream<tokio::net::TcpStream>>, soap: &str) {
    let resp = format!(
        "HTTP/1.1 200 OK\r\nServer: GenieACS/1.2.9\r\nSOAPServer: GenieACS/1.2.9\r\nContent-Type: text/xml; charset=\"utf-8\"\r\nContent-Length: {}\r\nConnection: keep-alive\r\n\r\n{}",
        soap.len(),
        soap
    );
    let _ = write_half.write_all(resp.as_bytes()).await;
    let _ = write_half.flush().await;
}

async fn send_204(write_half: &mut tokio::io::WriteHalf<SslStream<tokio::net::TcpStream>>) {
    let resp = "HTTP/1.1 204 No Content\r\nServer: GenieACS/1.2.9\r\nSOAPServer: GenieACS/1.2.9\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
    let _ = write_half.write_all(resp.as_bytes()).await;
    let _ = write_half.flush().await;
}

fn detect_dialect(xml: &str) -> SoapDialect {
    let env = if xml.contains("<soap-env:Envelope") {
        "soap-env"
    } else if xml.contains("<soap:Envelope") {
        "soap"
    } else {
        "SOAP-ENV"
    };
    let enc = if xml.contains("xmlns:soap-enc=") {
        "soap-enc"
    } else {
        "SOAP-ENC"
    };
    let cwmp_ns = if xml.contains("urn:dslforum-org:cwmp-1-2") {
        "urn:dslforum-org:cwmp-1-2"
    } else if xml.contains("urn:dslforum-org:cwmp-1-1") {
        "urn:dslforum-org:cwmp-1-1"
    } else {
        "urn:dslforum-org:cwmp-1-0"
    };
    let root = if xml.contains("InternetGatewayDevice.") {
        "InternetGatewayDevice"
    } else {
        "Device"
    };
    SoapDialect {
        env: env.to_string(),
        enc: enc.to_string(),
        cwmp_ns: cwmp_ns.to_string(),
        root: root.to_string(),
    }
}

fn extract_cwmp_id(xml: &str) -> &str {
    for tag in ["cwmp:ID", "<ID"] {
        if let Some(pos) = xml.find(tag) {
            if let Some(gt) = xml[pos..].find('>') {
                let rest = &xml[pos + gt + 1..];
                if let Some(lt) = rest.find('<') {
                    let id = rest[..lt].trim();
                    if !id.is_empty() {
                        return id;
                    }
                }
            }
        }
    }
    "0"
}

fn extract_tag_value<'a>(xml: &'a str, tag: &str) -> Option<&'a str> {
    let direct_open = format!("<{}>", tag);
    let direct_close = format!("</{}>", tag);
    if let Some(s) = xml.find(&direct_open) {
        let start = s + direct_open.len();
        if let Some(e) = xml[start..].find(&direct_close) {
            return Some(xml[start..start + e].trim());
        }
    }

    // Also match namespace-prefixed tags like <cwmp:FaultCode>
    let prefixed_suffix = format!(":{}>", tag);
    if let Some(s) = xml.find(&prefixed_suffix) {
        let start = s + prefixed_suffix.len();
        let close_suffix = format!(":{}>", tag);
        if let Some(e) = xml[start..].find(&close_suffix) {
            let val_end = xml[start..start + e].rfind('<').unwrap_or(e);
            return Some(xml[start..start + val_end].trim());
        }
    }
    None
}

fn extract_param_info_list(xml: &str) -> Vec<(String, bool)> {
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(pos) = rest.find("<ParameterInfoStruct>") {
        let after = &rest[pos + 21..];
        if let Some(end) = after.find("</ParameterInfoStruct>") {
            let block = &after[..end];
            if let Some(name) = extract_tag_value(block, "Name") {
                let w_str = extract_tag_value(block, "Writable").unwrap_or("0");
                let writable = w_str == "1" || w_str.eq_ignore_ascii_case("true");
                out.push((name.to_string(), writable));
            }
            rest = &after[end + 22..];
        } else {
            break;
        }
    }
    out
}

fn extract_param_values(xml: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(pos) = rest.find("<ParameterValueStruct>") {
        let after = &rest[pos + 22..];
        if let Some(end) = after.find("</ParameterValueStruct>") {
            let block = &after[..end];
            if let Some(name) = extract_tag_value(block, "Name") {
                let val = if let Some(vpos) = block.find("<Value") {
                    if let Some(gt) = block[vpos..].find('>') {
                        let vrest = &block[vpos + gt + 1..];
                        if let Some(lt) = vrest.find("</Value>") {
                            vrest[..lt].trim()
                        } else {
                            ""
                        }
                    } else {
                        ""
                    }
                } else {
                    ""
                };
                out.push((name.to_string(), val.to_string()));
            }
            rest = &after[end + 23..];
        } else {
            break;
        }
    }
    out
}

fn inform_response_xml(d: &SoapDialect, id: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><{env}:Envelope xmlns:{env}="http://schemas.xmlsoap.org/soap/envelope/" xmlns:{enc}="http://schemas.xmlsoap.org/soap/encoding/" xmlns:xsd="http://www.w3.org/2001/XMLSchema" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xmlns:cwmp="{ns}"><{env}:Header><cwmp:ID {env}:mustUnderstand="1">{id}</cwmp:ID></{env}:Header><{env}:Body><cwmp:InformResponse><MaxEnvelopes>1</MaxEnvelopes></cwmp:InformResponse></{env}:Body></{env}:Envelope>"#,
        env = d.env,
        enc = d.enc,
        ns = d.cwmp_ns,
        id = id
    )
}

fn get_param_names_xml(d: &SoapDialect, id: &str, path: &str, next_level: bool) -> String {
    let nl = if next_level { "1" } else { "0" };
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><{env}:Envelope xmlns:{env}="http://schemas.xmlsoap.org/soap/envelope/" xmlns:{enc}="http://schemas.xmlsoap.org/soap/encoding/" xmlns:xsd="http://www.w3.org/2001/XMLSchema" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xmlns:cwmp="{ns}"><{env}:Header><cwmp:ID {env}:mustUnderstand="1">{id}</cwmp:ID></{env}:Header><{env}:Body><cwmp:GetParameterNames><ParameterPath>{path}</ParameterPath><NextLevel>{nl}</NextLevel></cwmp:GetParameterNames></{env}:Body></{env}:Envelope>"#,
        env = d.env,
        enc = d.enc,
        ns = d.cwmp_ns,
        id = id,
        path = path,
        nl = nl
    )
}

fn set_single_param_xml(d: &SoapDialect, id: &str, name: &str, xsd_type: &str, value: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><{env}:Envelope xmlns:{env}="http://schemas.xmlsoap.org/soap/envelope/" xmlns:{enc}="http://schemas.xmlsoap.org/soap/encoding/" xmlns:xsd="http://www.w3.org/2001/XMLSchema" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xmlns:cwmp="{ns}"><{env}:Header><cwmp:ID {env}:mustUnderstand="1">{id}</cwmp:ID></{env}:Header><{env}:Body><cwmp:SetParameterValues><ParameterList {enc}:arrayType="cwmp:ParameterValueStruct[1]"><ParameterValueStruct><Name>{name}</Name><Value xsi:type="{xsd_type}">{value}</Value></ParameterValueStruct></ParameterList><ParameterKey></ParameterKey></cwmp:SetParameterValues></{env}:Body></{env}:Envelope>"#,
        env = d.env,
        enc = d.enc,
        ns = d.cwmp_ns,
        id = id,
        name = name,
        xsd_type = xsd_type,
        value = value
    )
}
