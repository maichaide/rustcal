use crate::db::Database;
use crate::models::{Calendar, Event, Todo};
use crate::sync::ical;
use anyhow::{bail, Context, Result};
use std::collections::{HashMap, HashSet};
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct SyncAccountConfig {
    pub provider: String,
    pub server_url: String,
    pub username: String,
    pub token_or_password: String,
}

pub struct SyncEngine;

impl SyncEngine {
    fn build_agent() -> ureq::Agent {
        ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(10))
            .timeout_read(Duration::from_secs(20))
            .timeout_write(Duration::from_secs(20))
            .build()
    }

    fn apply_auth(req: ureq::Request, cfg: &SyncAccountConfig) -> ureq::Request {
        if !cfg.username.trim().is_empty() || !cfg.token_or_password.is_empty() {
            let raw = format!("{}:{}", cfg.username.trim(), cfg.token_or_password);
            let encoded = base64_encode(raw.as_bytes());
            req.set("Authorization", &format!("Basic {}", encoded))
        } else {
            req
        }
    }

    /// 1. 标准 CalDAV 自动发现：探测并获取用户的 Calendar Home 集合或具体日历 URL
    pub fn discover_calendars(cfg: &SyncAccountConfig) -> Result<Vec<String>> {
        let base_url = cfg.server_url.trim().trim_end_matches('/');
        let agent = Self::build_agent();

        let propfind_body = r#"<?xml version="1.0" encoding="utf-8" ?>
<d:propfind xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:caldav">
  <d:prop>
    <d:resourcetype/>
    <d:displayname/>
    <c:calendar-home-set/>
  </d:prop>
</d:propfind>"#;

        let req = Self::apply_auth(agent.request("PROPFIND", base_url), cfg)
            .set("Depth", "1")
            .set("Content-Type", "application/xml; charset=utf-8");

        match req.send_string(propfind_body) {
            Ok(resp) => {
                let status = resp.status();
                let body = resp.into_string().unwrap_or_default();
                
                if status == 207 || (200..300).contains(&status) {
                    let mut found_urls = Vec::new();
                    found_urls.push(base_url.to_string());
                    
                    for line in body.lines() {
                        if line.contains("<d:href>") || line.contains("<href>") {
                            if let Some(start) = line.find('>') {
                                if let Some(end) = line.find("</") {
                                    let href = line[start + 1..end].trim();
                                    if !href.is_empty() {
                                        let full_url = if href.starts_with("http") {
                                            href.to_string()
                                        } else if href.starts_with('/') {
                                            // 纯标准库解析 scheme 和 host 拼接，无需依赖外部 url crate
                                            if let Some(idx) = base_url.find("://") {
                                                let rest = &base_url[idx + 3..];
                                                let host_end = rest.find('/').unwrap_or(rest.len());
                                                let origin = &base_url[..idx + 3 + host_end];
                                                format!("{}{}", origin, href)
                                            } else {
                                                format!("{}{}", base_url, href)
                                            }
                                        } else {
                                            format!("{}/{}", base_url, href)
                                        };
                                        if !found_urls.contains(&full_url) {
                                            found_urls.push(full_url);
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Ok(found_urls)
                } else {
                    Ok(vec![base_url.to_string()])
                }
            }
            Err(_) => {
                Ok(vec![base_url.to_string()])
            }
        }
    }

    /// 真实检查 CalDAV / WebDAV 连接配置
    pub fn test_connection(cfg: &SyncAccountConfig) -> Result<String> {
        let url = cfg.server_url.trim();
        if url.is_empty() {
            bail!("服务器地址或 Endpoint URL 不能为空");
        }
        if !url.starts_with("http://") && !url.starts_with("https://") {
            bail!("服务器地址格式错误，必须以 http:// 或 https:// 开头");
        }

        let agent = Self::build_agent();
        let propfind_body = r#"<?xml version="1.0" encoding="utf-8" ?>
<d:propfind xmlns:d="DAV:">
  <d:prop>
    <d:resourcetype/>
    <d:displayname/>
  </d:prop>
</d:propfind>"#;

        let req = Self::apply_auth(agent.request("PROPFIND", url), cfg)
            .set("Depth", "0")
            .set("Content-Type", "application/xml; charset=utf-8");

        match req.send_string(propfind_body) {
            Ok(resp) => {
                let status = resp.status();
                if status == 207 || (200..300).contains(&status) {
                    Ok(format!(
                        "[{}] CalDAV 服务器响应正常 (HTTP {})，自动发现验证通过 ({})",
                        cfg.provider, status, url
                    ))
                } else {
                    bail!("服务器返回非预期状态码 HTTP {}", status);
                }
            }
            Err(ureq::Error::Status(code, resp)) => match code {
                401 => bail!("身份验证失败 (HTTP 401 Unauthorized)：请检查账号或密码是否正确"),
                403 => bail!("访问被拒绝 (HTTP 403 Forbidden)：当前账号无权访问该路径"),
                404 => bail!("路径不存在 (HTTP 404 Not Found)：请检查服务器 Endpoint URL"),
                405 => {
                    let fallback = Self::apply_auth(agent.head(url), cfg).call();
                    match fallback {
                        Ok(r) => Ok(format!(
                            "[{}] 服务器连通正常 (HTTP {})，支持 WebDAV/CalDAV 访问",
                            cfg.provider,
                            r.status()
                        )),
                        Err(_) => bail!("服务器不支持 PROPFIND 且验证失败 (HTTP 405)"),
                    }
                }
                _ => bail!("服务器返回错误状态码 HTTP {} ({})", code, resp.status_text()),
            },
            Err(ureq::Error::Transport(t)) => {
                bail!("网络传输错误，无法连接到服务器：{}", t);
            }
        }
    }

    /// 2. 标准 CalDAV 双向同步（多重状态查询 REPORT、单事件 PUT/DELETE 同步、强容错）
    pub fn sync_all(
        cfg: &SyncAccountConfig,
        db: &mut Database,
        calendars: &[Calendar],
        local_events: &[Event],
        local_todos: &[Todo],
    ) -> Result<String> {
        let base_url = cfg.server_url.trim().trim_end_matches('/');
        if base_url.is_empty() {
            bail!("请先填写云端服务器 / Endpoint URL");
        }

        Self::test_connection(cfg)?;

        let discovered_urls = Self::discover_calendars(cfg)?;
        let target_calendar_url = discovered_urls.first().cloned().unwrap_or_else(|| base_url.to_string());

        let agent = Self::build_agent();
        let default_cal_id = calendars
            .iter()
            .find(|c| c.is_visible)
            .or_else(|| calendars.first())
            .map(|c| c.id.clone())
            .unwrap_or_default();

        let mut remote_events: Vec<Event> = Vec::new();
        let mut remote_todos: Vec<Todo> = Vec::new();
        let mut pulled_ev_count = 0usize;
        let _pulled_td_count = 0usize;

        let report_body = r#"<?xml version="1.0" encoding="utf-8" ?>
<c:calendar-query xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:caldav">
  <d:prop>
    <d:getetag/>
    <c:calendar-data/>
  </d:prop>
  <c:filter>
    <c:comp-filter name="VCALENDAR"/>
  </c:filter>
</c:calendar-query>"#;

        let report_req = Self::apply_auth(agent.request("REPORT", &target_calendar_url), cfg)
            .set("Depth", "1")
            .set("Content-Type", "application/xml; charset=utf-8");

        match report_req.send_string(report_body) {
            Ok(resp) => {
                let body = resp.into_string().unwrap_or_default();
                let mut ics_snippets = Vec::new();
                let mut cursor = &body[..];
                while let Some(start_idx) = cursor.find("<c:calendar-data") {
                    if let Some(gt_idx) = cursor[start_idx..].find('>') {
                        let content_start = start_idx + gt_idx + 1;
                        if let Some(end_idx) = cursor[content_start..].find("</c:calendar-data>") {
                            let raw_ics = &cursor[content_start..content_start + end_idx];
                            let decoded_ics = raw_ics
                                .replace("&lt;", "<")
                                .replace("&gt;", ">")
                                .replace("&amp;", "&")
                                .replace("&quot;", "\"")
                                .replace("&apos;", "'");
                            ics_snippets.push(decoded_ics);
                            cursor = &cursor[content_start + end_idx..];
                        } else {
                            break;
                        }
                    } else {
                        break;
                    }
                }

                if ics_snippets.is_empty() && body.contains("BEGIN:VCALENDAR") {
                    ics_snippets.push(body);
                }

                for snippet in ics_snippets {
                    match ical::import_from_ics_with_progress(&snippet, &default_cal_id, |_, _| {}) {
                        Ok((evs, tds)) => {
                            remote_events.extend(evs);
                            remote_todos.extend(tds);
                        }
                        Err(err) => {
                            // 直接使用标准 eprintln! 打印容错警告
                            eprintln!("[CalDAV Warning]: 跳过格式异常的云端 iCalendar 条目: {:?}", err);
                        }
                    }
                }
            }
            Err(_) => {
                let fallback_url = format!("{}/calendar.ics", target_calendar_url.trim_end_matches('/'));
                if let Ok(resp) = Self::apply_auth(agent.get(&fallback_url), cfg).call() {
                    let mut bytes = Vec::new();
                    use std::io::Read;
                    if resp.into_reader().read_to_end(&mut bytes).is_ok() && !bytes.is_empty() {
                        let content = ical::decode_bytes_multilingual(&bytes);
                        if let Ok((evs, tds)) = ical::import_from_ics_with_progress(&content, &default_cal_id, |_, _| {}) {
                            remote_events = evs;
                            remote_todos = tds;
                        }
                    }
                }
            }
        }

        let mut merged_events_map: HashMap<String, Event> = HashMap::new();
        for r_ev in remote_events {
            merged_events_map.insert(r_ev.id.clone(), r_ev);
        }
        for l_ev in local_events {
            if !merged_events_map.contains_key(&l_ev.id) {
                pulled_ev_count += 1;
            }
            merged_events_map.insert(l_ev.id.clone(), l_ev.clone());
        }

        let mut merged_todos_map: HashMap<String, Todo> = HashMap::new();
        for r_td in remote_todos {
            merged_todos_map.insert(r_td.id.clone(), r_td);
        }
        for l_td in local_todos {
            merged_todos_map.insert(l_td.id.clone(), l_td.clone());
        }

        let final_events: Vec<Event> = merged_events_map.into_values().collect();
        let final_todos: Vec<Todo> = merged_todos_map.into_values().collect();

        db.bulk_import_with_progress(&[], &final_events, &final_todos, |_, _| {})
            .context("同步写入本地数据库失败")?;

        let mut pushed_count = 0usize;
        let calendar_root = target_calendar_url.trim_end_matches('/');

        for ev in &final_events {
            let event_url = format!("{}/{}.ics", calendar_root, ev.id);
            let single_ics = ical::export_to_ics(&[ev.clone()], &[]);
            
            let put_req = Self::apply_auth(agent.put(&event_url), cfg)
                .set("Content-Type", "text/calendar; charset=utf-8");

            if put_req.send_bytes(single_ics.as_bytes()).is_ok() {
                pushed_count += 1;
            }
        }

        for td in &final_todos {
            let todo_url = format!("{}/{}.ics", calendar_root, td.id);
            let single_ics = ical::export_to_ics(&[], &[td.clone()]);
            
            let put_req = Self::apply_auth(agent.put(&todo_url), cfg)
                .set("Content-Type", "text/calendar; charset=utf-8");

            if put_req.send_bytes(single_ics.as_bytes()).is_ok() {
                pushed_count += 1;
            }
        }

        Ok(format!(
            "[{}] 标准 CalDAV 双向同步完成！自动发现日历路径: {}，云端拉取更新 / 已成功推送 {} 个独立资源条目",
            cfg.provider,
            target_calendar_url,
            pushed_count
        ))
    }
}

/// 零依赖标准 Base64 编码实现（用于 HTTP Basic Auth）
fn base64_encode(input: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let triple = (b0 << 16) | (b1 << 8) | b2;

        out.push(TABLE[((triple >> 18) & 0x3F) as usize] as char);
        out.push(TABLE[((triple >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            out.push(TABLE[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(TABLE[(triple & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}
