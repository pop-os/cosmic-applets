#[derive(Copy, Clone, PartialEq, Eq, Default, Debug, PartialOrd, Ord)]
pub struct HwAddress {
    address: u64,
}

impl HwAddress {
    pub fn from_str(arg: &str) -> Option<Self> {
        let columnless_vec = arg.split(":").collect::<Vec<&str>>();
        if columnless_vec.len() * 3 - 1 != arg.len() {
            return None;
        }
        for byte in &columnless_vec {
            if byte.len() != 2 {
                return None;
            }
        }
        u64::from_str_radix(columnless_vec.join("").as_str(), 16)
            .ok()
            .and_then(|address| Some(HwAddress { address }))
    }
    pub fn from_string(arg: &String) -> Option<Self> {
        HwAddress::from_str(arg.as_str())
    }
    pub fn to_string(&self) -> String {
        // return if self.address > 100000000000000 {
        //     "Intel Corp".to_string()
        // } else {
        //     "TP-Link".to_string()
        // };
        format!("{:#x}", self.address)
            .trim_start_matches("0x")
            .chars()
            .collect::<Vec<_>>()
            .chunks(2)
            .map(|chunk| chunk.iter().cloned().collect::<String>())
            .collect::<Vec<String>>()
            .join(":")
    }
}
