//! Fallback client for the old HTML/XML admin API
//! (`http://host:port/admin?cmd=...`).
//!
//! Used when the modern JSON-RPC endpoint is unavailable (older
//! PeerCast forks). See `docs/protocols/peercast.md` §3.2.

use crate::util::{
    errors::{AppError, AppResult},
    http::CLIENT,
};
use quick_xml::events::Event;
use quick_xml::reader::Reader;

use super::types::{ChannelInfo, ChannelRecord, ChannelStatus, PeerCastEndpoint, Track};

async fn admin(endpoint: &PeerCastEndpoint, query: &str) -> AppResult<String> {
    let url = format!("{}/admin?{query}", endpoint.base_url());
    let mut builder = CLIENT.get(&url);
    if let Some(auth) = &endpoint.auth {
        builder = builder.basic_auth(&auth.user, Some(&auth.pass));
    }
    let resp = builder.send().await?;
    if !resp.status().is_success() {
        return Err(AppError::Network(format!("admin returned {}", resp.status())));
    }
    Ok(resp.text().await?)
}

pub async fn stop(endpoint: &PeerCastEndpoint, channel_id: &str) -> AppResult<()> {
    admin(endpoint, &format!("cmd=stop&id={channel_id}")).await.map(drop)
}

pub async fn bump(endpoint: &PeerCastEndpoint, channel_id: &str) -> AppResult<()> {
    admin(endpoint, &format!("cmd=bump&id={channel_id}")).await.map(drop)
}

pub async fn view_xml(endpoint: &PeerCastEndpoint) -> AppResult<Vec<ChannelRecord>> {
    let body = admin(endpoint, "cmd=viewxml").await?;
    parse_view_xml(&body)
}

/// Parse the `<peercast>` XML structure returned by `cmd=viewxml`.
/// See `docs/protocols/peercast.md` §3.2 for the schema.
pub fn parse_view_xml(body: &str) -> AppResult<Vec<ChannelRecord>> {
    let mut reader = Reader::from_str(body);
    reader.config_mut().trim_text(true);

    let mut out: Vec<ChannelRecord> = Vec::new();
    let mut current: Option<ChannelRecord> = None;

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                match name.as_str() {
                    "channel" => {
                        let mut info = ChannelInfo::default();
                        let mut id = String::new();
                        for attr in e.attributes().flatten() {
                            let key = String::from_utf8_lossy(attr.key.as_ref()).into_owned();
                            let val =
                                attr.unescape_value().map(|c| c.into_owned()).unwrap_or_default();
                            match key.as_str() {
                                "name" => info.name = val,
                                "id" => id = val,
                                "bitrate" => info.bitrate = val.parse().unwrap_or(0),
                                "type" => info.content_type = val,
                                "genre" => info.genre = val,
                                "desc" => info.desc = val,
                                "url" => info.url = val,
                                "comment" => info.comment = val,
                                _ => {}
                            }
                        }
                        current =
                            Some(ChannelRecord { channel_id: id, info, ..Default::default() });
                    }
                    "track" => {
                        if let Some(rec) = current.as_mut() {
                            let mut t = Track::default();
                            for attr in e.attributes().flatten() {
                                let key = String::from_utf8_lossy(attr.key.as_ref()).into_owned();
                                let val = attr
                                    .unescape_value()
                                    .map(|c| c.into_owned())
                                    .unwrap_or_default();
                                match key.as_str() {
                                    "title" => t.name = val,
                                    "artist" => t.creator = val,
                                    "album" => t.album = val,
                                    "genre" => t.genre = val,
                                    "contact" => t.url = val,
                                    _ => {}
                                }
                            }
                            rec.track = t;
                        }
                    }
                    "relay" => {
                        if let Some(rec) = current.as_mut() {
                            let mut st = ChannelStatus::default();
                            for attr in e.attributes().flatten() {
                                let key = String::from_utf8_lossy(attr.key.as_ref()).into_owned();
                                let val = attr
                                    .unescape_value()
                                    .map(|c| c.into_owned())
                                    .unwrap_or_default();
                                match key.as_str() {
                                    "listeners" => st.local_directs = val.parse().unwrap_or(0),
                                    "relays" => st.local_relays = val.parse().unwrap_or(0),
                                    "status" => st.status = val,
                                    _ => {}
                                }
                            }
                            rec.status = st;
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::End(e)) => {
                if e.name().as_ref() == b"channel" {
                    if let Some(rec) = current.take() {
                        out.push(rec);
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(AppError::Decode(format!("XML parse error: {e}"))),
            _ => {}
        }
    }

    // For self-closing `<channel ... />` cases we already inserted on Empty.
    if let Some(rec) = current.take() {
        out.push(rec);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<peercast session_id="00000000000000000000000000000000">
  <channels_found total="2">
    <channel name="Ch1" id="0123456789ABCDEF0123456789ABCDEF"
             bitrate="320" type="FLV" genre="Music" desc="d" url="http://x/" comment="c">
      <relay listeners="5" relays="2" hosts="3" status="Receiving" firewalled="0" />
      <track title="Song" artist="Artist" album="Alb" genre="G" contact="http://t/" />
    </channel>
    <channel name="Ch2" id="ABCDEF0123456789ABCDEF0123456789"
             bitrate="128" type="WMV" genre="" desc="" url="" comment="">
      <relay listeners="0" relays="0" hosts="0" status="Idle" firewalled="0" />
      <track title="" artist="" album="" genre="" contact="" />
    </channel>
  </channels_found>
</peercast>"#;

    #[test]
    fn parses_two_channels() {
        let v = parse_view_xml(XML).unwrap();
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].info.name, "Ch1");
        assert_eq!(v[0].info.bitrate, 320);
        assert_eq!(v[0].info.content_type, "FLV");
        assert_eq!(v[0].track.name, "Song");
        assert_eq!(v[0].track.creator, "Artist");
        assert_eq!(v[0].status.status, "Receiving");
        assert_eq!(v[0].status.local_directs, 5);
        assert_eq!(v[0].status.local_relays, 2);
        assert_eq!(v[1].info.name, "Ch2");
        assert_eq!(v[1].info.bitrate, 128);
    }

    #[test]
    fn empty_peercast_root_is_ok() {
        let xml = "<peercast/>";
        assert!(parse_view_xml(xml).unwrap().is_empty());
    }

    #[test]
    fn malformed_xml_errors() {
        assert!(parse_view_xml("<peercast><channel name='x'></peercast>").is_err());
    }
}
