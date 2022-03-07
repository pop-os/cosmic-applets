// SPDX-License-Identifier: LGPL-3.0-or-later

#[macro_use]
extern crate relm4;

mod app;

use once_cell::sync::Lazy;
use tokio::runtime::Runtime;

static RT: Lazy<Runtime> = Lazy::new(|| Runtime::new().expect("failed to build tokio runtime"));

fn main() {
    println!("Hello, world!");
}
