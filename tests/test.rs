//! Tests

#[cfg(test)]
mod tests {
    use galion::librclone::rclone::Rclone;

    #[test]
    fn test_get_config() {
        let mut rclone = Rclone::default();
        rclone.initialize();
        let res = rclone.get_rpc_config().unwrap();
        println!("{}", serde_json::to_string_pretty(&res).unwrap());
        rclone.finalize();
    }
}
