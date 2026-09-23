use gpui_shell::{HostModule, HostValue};

const DECLARATIONS: &str = r#"
export function greet(name: string): string;
"#;

pub fn module() -> HostModule {
  HostModule::new("corona")
    .declarations(DECLARATIONS)
    .function("greet", |args| {
      Ok(HostValue::from(format!(
        "Hello {}, from Rust",
        args.string(0)?
      )))
    })
}
