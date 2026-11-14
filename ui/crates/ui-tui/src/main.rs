use ui_model::{DemoBackend, Role};

struct Options {
    role: Role,
    demo: bool,
    api_url: String,
}

impl Options {
    fn parse(args: &[String], environment_url: Option<String>) -> Result<Self, String> {
        let mut options = Self {
            role: Role::Crew,
            demo: false,
            api_url: environment_url.unwrap_or_else(|| "http://127.0.0.1:8080".into()),
        };
        let mut api_argument = false;
        let mut args = args.iter();
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--ground" => options.role = Role::Ground,
                "--demo" => options.demo = true,
                "--api-url" => {
                    if api_argument {
                        return Err("--api-url may only be supplied once".into());
                    }
                    options.api_url = args
                        .next()
                        .filter(|v| !v.starts_with("--"))
                        .ok_or("--api-url requires a server URL")?
                        .clone();
                    api_argument = true;
                }
                _ => return Err(format!("Unknown argument: {arg}. Use --help for usage.")),
            }
        }
        if options.demo && api_argument {
            return Err("Choose --demo or --api-url; they use separate data sources".into());
        }
        Ok(options)
    }
}

fn main() -> std::io::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!(
            "astrocheck-tui [--ground] [--api-url URL | --demo]\n\
  Default: shared Rust API at http://127.0.0.1:8080\n\
  --api-url URL  override ASTROCHECK_API_URL and the default server\n\
  --ground       start in the ground console\n\
  --demo         isolated in-memory demo; does not access the API\n\
  Live keys: arrows select, Enter edits a metric, Tab selects tasks, Space toggles\n\
  a task, i shows details, h health guidance, f flight hazards, r role, ? help, q quits. Use an 80x24 UTF-8 terminal."
        );
        return Ok(());
    }
    let opts = Options::parse(&args, std::env::var("ASTROCHECK_API_URL").ok())
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?;
    if opts.demo {
        return ui_tui::run_as(DemoBackend::new(), opts.role);
    }
    let backend = ui_http::HttpBackend::connect(&opts.api_url)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?;
    ui_tui::run_as(backend, opts.role)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|s| (*s).into()).collect()
    }

    #[test]
    fn defaults_to_shared_http_and_cli_overrides_environment() {
        let default = Options::parse(&[], None).unwrap();
        assert!(!default.demo);
        assert_eq!(default.api_url, "http://127.0.0.1:8080");
        let explicit = Options::parse(
            &args(&["--api-url", "http://pi:8080", "--ground"]),
            Some("http://other:8080".into()),
        )
        .unwrap();
        assert_eq!(explicit.api_url, "http://pi:8080");
        assert_eq!(explicit.role, Role::Ground);
        let env = Options::parse(&[], Some("http://env:8080".into())).unwrap();
        assert_eq!(env.api_url, "http://env:8080");
    }

    #[test]
    fn rejects_ambiguous_or_invalid_cli_before_opening_the_terminal() {
        for values in [
            &["--api-url"][..],
            &["--api-url", "--ground"],
            &["--demo", "--api-url", "http://host"],
            &["--bad"],
            &["--api-url", "http://a", "--api-url", "http://b"],
        ] {
            assert!(Options::parse(&args(values), None).is_err());
        }
        assert!(
            Options::parse(&args(&["--demo"]), Some("http://env:8080".into()))
                .unwrap()
                .demo
        );
    }
}
