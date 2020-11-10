mod node;
mod message;
mod broadcaster;

#[cfg(test)]
mod tests {
    use super::broadcaster::Broadcaster;
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }

    #[test]
    fn blah() {
        let b: Broadcaster = Broadcaster::new();
        assert!(b.broadcast("Hello").is_ok())
    }
}
