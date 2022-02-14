use std::path::PathBuf;
use getopts::Options;
use anyhow::anyhow;

/// Holds various configuration options.
pub struct Config {
    pub has_infile: bool,
    pub infile_name: PathBuf,
    pub ro: bool
}

/// Parses the cmdline options and returns Config.
pub fn parse_options() -> anyhow::Result<Config>
{
    let argv: Vec<String> = std::env::args().collect();

    let mut options = Options::new();

    options.optflag("h", "help", "display help");

    let present_options = match options.parse(&argv[1..]) {
        Ok(o) => o,
        Err(e) => {
            return Err(anyhow!("{}", &e));
        }
    };

    // Initiate the return config with default values.
    let mut config = Config {
        has_infile: false,
        infile_name: PathBuf::default(),
        ro: false
    };

    if present_options.opt_present("h") {
            usage();
            std::process::exit(0);
    };

    // Get the non-option arg. (file name)
    if !present_options.free.is_empty() {
        config.infile_name = PathBuf::from(&present_options.free[0]);
        config.has_infile = true;
    };

    // (The Rust Foundation, 2019)

    Ok(config)
}

/// Prints the usage.
pub fn usage()
{
    let argv: Vec<String> = std::env::args().collect();

    eprintln!("Usage: {} [OPTION]... FILE", argv[0]);
    eprintln!();
    eprintln!("Options:");
    eprintln!("  -h, --help  display help");
}
