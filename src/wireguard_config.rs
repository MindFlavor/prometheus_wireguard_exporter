use std::collections::HashMap;

#[derive(Debug, Default, Clone)]
pub(crate) struct PeerEntry {
    pub public_key: String,
    pub name: Option<String>,
}

pub(crate) fn parse<'a>(txt: &'a str) -> HashMap<String, PeerEntry> {
    let mut ht = HashMap::new();

    let mut name = "";
    txt.lines().fold("", |prev, cur| {
        if cur == "[Peer]" {
            if prev.chars().next() == Some('#') {
                name = prev;
            }
        } else if cur.starts_with("PublicKey") {
            // public key found, use it as key
            // TODO we must strip the PublicKey = first !
            ht.insert(
                cur.to_owned(),
                PeerEntry {
                    public_key: cur.to_owned(),
                    name: Some(name.to_owned()),
                },
            );
        }

        cur
    });

    ht
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEXT: &'static str = "
ListenPort = 51820
PrivateKey = my_super_secret_private_key
# PreUp = iptables -t nat -A POSTROUTING -s 10.70.0.0/24  -o enp7s0 -j MASQUERADE
# PostDown = iptables -t nat -D POSTROUTING -s 10.70.0.0/24  -o enp7s0 -j MASQUERADE

# OnePlus 6T
[Peer]
PublicKey = 2S7mA0vEMethCNQrJpJKE81/JmhgtB+tHHLYQhgM6kk=
AllowedIPs = 10.70.0.2/32

# varch.local (laptop)
[Peer]
PublicKey = qnoxQoQI8KKMupLnSSureORV0wMmH7JryZNsmGVISzU=
AllowedIPs = 10.70.0.3/32

# cantarch
[Peer]
PublicKey = L2UoJZN7RmEKsMmqaJgKG0m1S2Zs2wd2ptAf+kb3008=
AllowedIPs = 10.70.0.4/32

# frcognoarch
[Peer]
PublicKey = MdVOIPKt9K2MPj/sO2NlWQbOnFJ6L/qX80mmhQwsUlA=
AllowedIPs = 10.70.0.50/32

# frcognowin10
[Peer]
PublicKey = lqYcojJMsIZXMUw1heAFbQHBoKjCEaeo7M1WXDh/KWc=
AllowedIPs = 10.70.0.40/32

# OnePlus 5T
[Peer]
PublicKey = 928vO9Lf4+Mo84cWu4k1oRyzf0AR7FTGoPKHGoTMSHk=
AllowedIPs = 10.70.0.80/32
";

    #[test]
    fn test_parse() {
        let a = parse(TEXT);
        println!("{:?}", a);
    }

    #[test]
    fn test_parse_and_serialize() {}
}
