// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use std::error::Error;
use std::fmt::{self, Display, Formatter};

#[derive(Debug)]
pub enum Never {}

impl Display for Never {
    fn fmt(&self, _: &mut Formatter) -> fmt::Result {
        unreachable!();
    }
}

impl Error for Never {
    fn description(&self) -> &str {
        unreachable!();
    }
}
