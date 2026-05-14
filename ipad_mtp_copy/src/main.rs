use anyhow::Result;

fn main() -> Result<()> {
    wpd_enum::list_dcim()?; // call into the library
    Ok(())
}
