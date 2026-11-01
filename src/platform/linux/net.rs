//! Sockets from `ss`, with each socket's process taken from the `users:` column.

use super::host;
use crate::sysops::Connection;

pub fn connections() -> Vec<Connection> {
    host("ss", &["-tunapH"])
        .map(|out| parse(&out))
        .unwrap_or_default()
}

/// `ss -tunapH` lines: netid, state, recv-q, send-q, local, peer, then `users:(("name",pid=N,fd=M))`.
fn parse(out: &str) -> Vec<Connection> {
    out.lines().filter_map(parse_line).collect()
}

fn parse_line(line: &str) -> Option<Connection> {
    let fields: Vec<&str> = line.split_whitespace().collect();
    if fields.len() < 6 {
        return None;
    }
    let (proto, state, local, peer) = (fields[0], fields[1], fields[4], fields[5]);
    let (process, pid) = fields[6..]
        .iter()
        .find_map(|f| owner(f))
        .unwrap_or_default();
    let endpoint = if matches!(peer, "*:*" | "0.0.0.0:*" | "[::]:*") {
        local.to_string()
    } else {
        format!("{local}->{peer}")
    };
    Some(Connection {
        pid,
        process,
        proto: proto.to_uppercase(),
        endpoint,
        state: state_name(state),
    })
}

/// The first process of a `users:(("name",pid=N,fd=M),…)` field.
fn owner(field: &str) -> Option<(String, u32)> {
    let rest = field.strip_prefix("users:((")?;
    let name = rest.split('"').nth(1)?.to_string();
    let pid = rest
        .split("pid=")
        .nth(1)?
        .split(|c: char| !c.is_ascii_digit())
        .next()?
        .parse()
        .ok()?;
    Some((name, pid))
}

/// The state names macOS shows for the same sockets (UDP has none).
fn state_name(state: &str) -> String {
    match state {
        "ESTAB" => "ESTABLISHED".into(),
        "UNCONN" => String::new(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SS: &str = "tcp ESTAB 0 0 192.168.1.20:54012 142.250.74.14:443 users:((\"chrome\",pid=1200,fd=41))\n\
tcp LISTEN 0 4096 127.0.0.1:631 0.0.0.0:* \n\
udp UNCONN 0 0 0.0.0.0:5353 0.0.0.0:* users:((\"avahi-daemon\",pid=777,fd=13))\n\
tcp ESTAB 0 0 [::1]:22 [::1]:50422 users:((\"sshd\",pid=2201,fd=4),(\"x\",pid=5,fd=1))\n";

    #[test]
    fn sockets_map_to_their_process() {
        let rows = parse(SS);
        assert_eq!(rows.len(), 4);
        assert_eq!(rows[0].process, "chrome");
        assert_eq!(rows[0].pid, 1200);
        assert_eq!(rows[0].proto, "TCP");
        assert_eq!(rows[0].state, "ESTABLISHED");
        assert_eq!(rows[0].endpoint, "192.168.1.20:54012->142.250.74.14:443");
    }

    #[test]
    fn listeners_show_only_their_local_end() {
        let rows = parse(SS);
        assert_eq!(rows[1].endpoint, "127.0.0.1:631");
        assert_eq!(rows[1].state, "LISTEN");
        assert_eq!(rows[2].proto, "UDP");
        assert_eq!(rows[2].state, "");
    }

    #[test]
    fn the_first_owner_wins_and_missing_owners_are_zero() {
        let rows = parse(SS);
        assert_eq!((rows[3].process.as_str(), rows[3].pid), ("sshd", 2201));
        let none = parse("tcp LISTEN 0 4096 *:80 *:* \n");
        assert_eq!((none[0].process.as_str(), none[0].pid), ("", 0));
    }
}
