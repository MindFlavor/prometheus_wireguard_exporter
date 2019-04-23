use crate::exporter_error::ExporterError;
use crate::render_to_prometheus::RenderToPrometheus;
use log::{debug, trace};
use std::collections::HashMap;
use std::convert::TryFrom;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const EMPTY: &str = "(none)";

#[derive(Default, Debug, Clone)]
pub(crate) struct LocalEndpoint {
    pub public_key: String,
    pub private_key: String,
    pub local_port: u32,
    pub persistent_keepalive: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct RemoteEndpoint {
    pub public_key: String,
    pub remote_ip: Option<String>,
    pub remote_port: Option<u32>,
    pub local_ip: String,
    pub local_subnet: String,
    pub latest_handshake: SystemTime,
    pub sent_bytes: u128,
    pub received_bytes: u128,
    pub persistent_keepalive: bool,
}

#[derive(Debug, Clone)]
pub(crate) enum Endpoint {
    Local(LocalEndpoint),
    Remote(RemoteEndpoint),
}

fn to_option_string(s: &str) -> Option<String> {
    if s == EMPTY {
        None
    } else {
        Some(s.to_owned())
    }
}

fn to_bool(s: &str) -> bool {
    s != "off"
}

#[derive(Debug, Clone)]
pub(crate) struct WireGuard {
    pub interfaces: HashMap<String, Vec<Endpoint>>,
}

impl TryFrom<&str> for WireGuard {
    type Error = ExporterError;

    fn try_from(input: &str) -> Result<Self, Self::Error> {
        debug!("wireguard::try_from({}) called", input);
        let mut wg = WireGuard {
            interfaces: HashMap::new(),
        };

        for line in input.lines() {
            let v: Vec<&str> = line.split('\t').filter(|s| !s.is_empty()).collect();
            debug!("v == {:?}", v);

            let endpoint = if v.len() == 5 {
                // this is the local interface
                Endpoint::Local(LocalEndpoint {
                    public_key: v[1].to_owned(),
                    private_key: v[2].to_owned(),
                    local_port: v[3].parse::<u32>().unwrap(),
                    persistent_keepalive: to_bool(v[4]),
                })
            } else {
                // remote endpoint
                let public_key = v[1].to_owned();

                let (remote_ip, remote_port) = if let Some(ip_and_port) = to_option_string(v[3]) {
                    let toks: Vec<&str> = ip_and_port.split(':').collect();
                    (
                        Some(toks[0].to_owned()),
                        Some(toks[1].parse::<u32>().unwrap()),
                    )
                } else {
                    (None, None)
                };

                let tok: Vec<&str> = v[4].split('/').collect();
                let (local_ip, local_subnet) = (tok[0].to_owned(), tok[1].to_owned());

                // the latest_handhshake is based on Linux representation: a tick is a second. So
                // the hack here is: add N seconds to the UNIX_EPOCH constant. This wil not work
                // on other platforms if the returned ticks are *not* seconds. Sadly I did not find
                // an alternative way to initialize a SystemTime from a tick number.
                Endpoint::Remote(RemoteEndpoint {
                    public_key,
                    remote_ip,
                    remote_port,
                    local_ip,
                    local_subnet,
                    latest_handshake: UNIX_EPOCH
                        .checked_add(Duration::from_secs(v[5].parse::<u64>()?))
                        .unwrap(),
                    sent_bytes: v[6].parse::<u128>().unwrap(),
                    received_bytes: v[7].parse::<u128>().unwrap(),
                    persistent_keepalive: to_bool(v[8]),
                })
            };

            trace!("{:?}", endpoint);

            if let Some(endpoints) = wg.interfaces.get_mut(v[0]) {
                endpoints.push(endpoint);
            } else {
                let mut new_vec = Vec::new();
                new_vec.push(endpoint);
                wg.interfaces.insert(v[0].to_owned(), new_vec);
            }
        }

        trace!("{:?}", wg);
        Ok(wg)
    }
}

impl RenderToPrometheus for WireGuard {
    fn render(&self) -> String {
        let mut latest_handshakes = Vec::new();
        let mut sent_bytes = Vec::new();
        let mut received_bytes = Vec::new();

        for (interface, endpoints) in self.interfaces.iter() {
            for endpoint in endpoints {
                // only show remote endpoints
                if let Endpoint::Remote(ep) = endpoint {
                    debug!("{:?}", ep);
                    sent_bytes.push(format!("wireguard_sent_bytes{{inteface=\"{}\", public_key=\"{}\", local_ip=\"{}\", local_subnet=\"{}\"}} {}\n", interface, ep.public_key, ep.local_ip, ep.local_subnet, ep.sent_bytes));
                    received_bytes.push(format!("wireguard_received_bytes{{inteface=\"{}\", public_key=\"{}\", local_ip=\"{}\", local_subnet=\"{}\"}} {}\n", interface, ep.public_key, ep.local_ip, ep.local_subnet, ep.received_bytes));
                    latest_handshakes.push(format!("wireguard_latest_handshake_seconds{{inteface=\"{}\", public_key=\"{}\", local_ip=\"{}\", local_subnet=\"{}\"}} {}\n", interface, ep.public_key, ep.local_ip, ep.local_subnet, ::std::time::SystemTime::now().duration_since(ep.latest_handshake).unwrap().as_secs()));
                }
            }
        }

        let mut s = String::new();

        s.push_str(
            "# HELP wireguard_sent_bytes Bytes sent to the peer
# TYPE wireguard_sent_bytes counter\n",
        );
        for peer in sent_bytes {
            s.push_str(&peer);
        }

        s.push_str(
            "# HELP wireguard_received_bytes Bytes received from the peer
# TYPE wireguard_received_bytes counter\n",
        );
        for peer in received_bytes {
            s.push_str(&peer);
        }

        s.push_str(
            "# HELP wireguard_latest_handshake_seconds Seconds from the last handshake
# TYPE wireguard_latest_handshake_seconds gauge\n",
        );
        for peer in latest_handshakes {
            s.push_str(&peer);
        }

        debug!("{}", s);
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEXT : &'static str = "wg0\t000q4qAC0ExW/BuGSmVR1nxH9JAXT6g9Wd3oEGy5lA=\t0000u8LWR682knVm350lnuqlCJzw5SNLW9Nf96P+m8=\t51820\toff
wg0\t2S7mA0vEMethCNQrJpJKE81/JmhgtB+tHHLYQhgM6kk=\t(none)\t37.159.76.245:29159\t10.70.0.2/32\t1555771458\t10288508\t139524160\toff
wg0\tqnoxQoQI8KKMupLnSSureORV0wMmH7JryZNsmGVISzU=\t(none)\t(none)\t10.70.0.3/32\t0\t0\t0\toff
wg0\tL2UoJZN7RmEKsMmqaJgKG0m1S2Zs2wd2ptAf+kb3008=\t(none)\t(none)\t10.70.0.4/32\t0\t0\t0\toff
wg0\tMdVOIPKt9K2MPj/sO2NlWQbOnFJ6L/qX80mmhQwsUlA=\t(none)\t(none)\t10.70.0.50/32\t0\t0\t0\toff
wg2\tMdVOIPKt9K2MPj/sO2NlWQbOnFJcL/qX80mmhQwsUlA=\t(none)\t(none)\t10.70.5.50/32\t0\t0\t0\toff
pollo\tYdVOIPKt9K2MPsO2NlWQbOnFJcL/qX80mmhQwsUlA=\t(none)\t(none)\t10.70.70.50/32\t0\t0\t0\toff
wg0\t928vO9Lf4+Mo84cWu4k1oRyzf0AR7FTGoPKHGoTMSHk=\t(none)\t5.90.62.106:21741\t10.70.0.80/32\t1555344925\t283012\t6604620\toff
";

    #[test]
    fn test_parse() {
        let a = WireGuard::try_from(TEXT).unwrap();
        println!("{:?}", a);
        assert!(a.interfaces.len() == 3);
        assert!(a.interfaces["wg0"].len() == 6);

        let e1 = match &a.interfaces["wg0"][1] {
            Endpoint::Local(_) => panic!(),
            Endpoint::Remote(re) => re,
        };

        assert!(e1.public_key == "2S7mA0vEMethCNQrJpJKE81/JmhgtB+tHHLYQhgM6kk=");
    }

    #[test]
    fn test_parse_and_serialize() {
        let a = WireGuard::try_from(TEXT).unwrap();
        let s = a.render();
        println!("{}", s);
    }
}
