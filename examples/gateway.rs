mod utils;

use smoltcp::iface::{Config, Interface, SocketSet};
use smoltcp::phy::{wait as phy_wait, Medium};
use smoltcp::socket::Socket;
use smoltcp::time::Instant;
use smoltcp::wire::{EthernetAddress, IpAddress, IpCidr, Ipv4Address, Ipv6Address};

fn main() {
    utils::setup_logging("");

    let (mut opts, mut free) = utils::create_options();
    utils::add_tuntap_options(&mut opts, &mut free);
    utils::add_middleware_options(&mut opts, &mut free);

    let mut matches = utils::parse_options(&opts, free);
    let device = utils::parse_tuntap_options(&mut matches);
    let fd = device.as_raw_fd();
    let mut device =
        utils::parse_middleware_options(&mut matches, device, /*loopback=*/ false);

    // Create interface
    let mut config = match device.capabilities().medium {
        Medium::Ethernet => {
            Config::new(EthernetAddress([0x02, 0x00, 0x00, 0x00, 0x00, 0x01]).into())
        }
        Medium::Ip => Config::new(smoltcp::wire::HardwareAddress::Ip),
        Medium::Ieee802154 => todo!(),
    };

    config.random_seed = rand::random();

    let mut iface = Interface::new(config, &mut device);
    iface.update_ip_addrs(|ip_addrs| {
        ip_addrs
            .push(IpCidr::new(IpAddress::v4(192, 168, 69, 1), 24))
            .unwrap();
        ip_addrs
            .push(IpCidr::new(IpAddress::v6(0xfdaa, 0, 0, 0, 0, 0, 0, 1), 64))
            .unwrap();
        ip_addrs
            .push(IpCidr::new(IpAddress::v6(0xfe80, 0, 0, 0, 0, 0, 0, 1), 64))
            .unwrap();
    });
    iface
        .routes_mut()
        .add_default_ipv4_route(Ipv4Address::new(192, 168, 69, 100))
        .unwrap();
    iface
        .routes_mut()
        .add_default_ipv6_route(Ipv6Address::new(0xfe80, 0, 0, 0, 0, 0, 0, 0x100))
        .unwrap();

    // Create sockets
    let mut sockets = SocketSet::new(vec![]);

    loop {
        let timestamp = Instant::now();
        iface.poll(timestamp, &mut device, &mut sockets);
        for (h, v) in sockets.iter_mut() {
            match v {
                Socket::Udp(v) => {
                    if v.can_recv() {
                        let (data, ep) = v.recv().unwrap();
                        let data = data.to_owned();
                        if v.can_send() && !data.is_empty() {
                            v.send_slice(&data[..], ep).unwrap();
                        }
                    }
                }
                Socket::Tcp(v) => {
                    if v.may_recv() {
                        let data = v
                            .recv(|buffer| {
                                let recvd_len = buffer.len();
                                let data = buffer.to_owned();
                                (recvd_len, data)
                            })
                            .unwrap();
                        if v.can_send() && !data.is_empty() {
                            v.send_slice(&data[..]).unwrap();
                        }
                    }
                }
                _ => (),
            }
        }
        phy_wait(fd, iface.poll_delay(timestamp, &sockets)).expect("wait error");
    }
}
