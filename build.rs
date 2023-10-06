use spirv_builder::SpirvBuilder;

fn main() -> Result<(), Box<dyn std::error::Error>> {
  SpirvBuilder::new("rt", "spirv-unknown-spv1.5").build()?;
  Ok(())
}
