fn main() -> Result<(), CappError> {
    multiobs_plugin::force_link();
    let dest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../pi/src/generated/contracts.ts");
    streamdeck_plugin::export_typescript(&dest)?;
    let source = std::fs::read_to_string(&dest)?;
    let exported = source.replace("\ntype ", "\nexport type ");
    std::fs::write(&dest, exported)?;
    println!("wrote {}", dest.display());
    Ok(())
}

#[derive(Debug)]
struct CappError(String);

impl std::fmt::Display for CappError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}

impl std::error::Error for CappError {}

impl From<streamdeck_plugin::Error> for CappError {
    fn from(error: streamdeck_plugin::Error) -> Self {
        Self(error.to_string())
    }
}

impl From<std::io::Error> for CappError {
    fn from(error: std::io::Error) -> Self {
        Self(error.to_string())
    }
}
