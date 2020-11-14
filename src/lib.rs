mod node;
mod message;
mod broadcaster;

#[cfg(test)]
mod tests {
    use std::{thread, time, net::{Ipv4Addr, SocketAddrV4}};
    use super::broadcaster::Broadcaster;
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }

    #[test]
    fn broadcast_init() {
        let mut b: Broadcaster = Broadcaster::new();
        assert!(b.open(8080).is_ok());
        //thread::sleep(time::Duration::from_millis(1000));
    }

    #[test]
    fn broadcast_send() {
        let b: Broadcaster = Broadcaster::new();
        assert!(b.broadcast("Hello").is_ok());
    }

    #[test]
    fn broadcast_send2() {
        let mut b: Broadcaster = Broadcaster::new();
        b.add_node(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 8080), None);
        assert!(b.broadcast("Hello").is_ok());
    }
}
