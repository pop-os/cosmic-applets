use std::fmt::Write;

#[derive(Copy, Clone, PartialEq, Eq, Default, Debug, PartialOrd, Ord)]
pub struct HwAddress {
    address: u64,
}

impl HwAddress {
    pub fn from_str(arg: &str) -> Option<Self> {
        let columnless_vec = arg.split(':').collect::<Box<[_]>>();
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
            .map(|address| HwAddress { address })
    }
}

impl std::fmt::Display for HwAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (index, c) in format!("{:x}", self.address).char_indices() {
            if index != 0 && index % 2 == 0 {
                f.write_char(':')?;
            }
            f.write_char(c)?;
        }

        Ok(())
    }
}
