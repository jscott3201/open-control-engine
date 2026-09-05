//! Export the live facade catalog or the packaged descriptors without a registry dependency.

use std::io::{self, Write};

fn main() -> io::Result<()> {
    let mut output = io::stdout().lock();
    if std::env::args().nth(1).as_deref() == Some("schemas") {
        for descriptor in oce_api::contract_descriptors() {
            output.write_all(descriptor.schema.as_bytes())?;
        }
    } else {
        output.write_all(oce_api::catalog_to_json(oce_api::catalog()).as_bytes())?;
    }
    Ok(())
}
