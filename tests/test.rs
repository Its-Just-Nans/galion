//! Tests

#[cfg(test)]
mod tests {
    use galion::librclone::rclone::Rclone;
    use std::{thread::sleep, time::Duration};

    #[test]
    fn test_get_config() {
        let mut rclone = Rclone::default();
        rclone.initialize();
        let res = rclone.get_rpc_config().unwrap();
        println!("{}", serde_json::to_string_pretty(&res).unwrap());
        sleep(Duration::from_secs(10));
        rclone.finalize();
    }

    #[test]
    fn test_get_job_list() {
        let mut rclone = Rclone::default();
        rclone.initialize();
        let res = rclone.get_rpc_config().unwrap();
        println!("{}", serde_json::to_string_pretty(&res).unwrap());
        let mut count = 0;
        while count < 20 {
            let res = rclone.job_list().unwrap();
            println!("{}", serde_json::to_string_pretty(&res).unwrap());
            sleep(Duration::from_secs(1));
            count += 1;
        }
        rclone.finalize();
    }
}
