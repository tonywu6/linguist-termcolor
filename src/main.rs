use clap::{Parser, Subcommand};
use color_art::{Color, ColorSpace};
use linguist_termcolor::{Linguist, TermColor};

fn main() -> anyhow::Result<()> {
    let Main {
        command,
        color_space,
    } = Main::parse();
    match command {
        Commands::Xterm { colors } => xterm(colors, color_space),
        Commands::Linguist { query } => linguist(query, color_space),
    }
}

fn xterm(colors: Vec<String>, color_space: ColorSpace) -> anyhow::Result<()> {
    for color in colors {
        let color = Color::from_hex(&color)?;
        let color = TermColor::from(color);
        println!("{}", color.print(color_space));
    }
    Ok(())
}

fn linguist(query: Vec<String>, color_space: ColorSpace) -> anyhow::Result<()> {
    let linguist = Linguist::new()?;
    let colors = linguist.colors()?;
    let found = colors.query(&query.join(" "));
    if found.is_empty() {
        Err(anyhow::anyhow!("no colors found for this language"))?
    }
    for (lang, color) in found {
        println!("{} {}", color.print(color_space), lang);
    }
    Ok(())
}

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Main {
    #[command(subcommand)]
    command: Commands,
    #[arg(
        short = 'c',
        long = "colors",
        default_value = "RGB",
        help = "The color model to be used for distance calculation. Default: RGB"
    )]
    color_space: ColorSpace,
}

#[derive(Subcommand, Debug)]
enum Commands {
    #[command(name = "for", about = "Query GitHub Linguist's language colors")]
    Linguist {
        #[arg(required = true, trailing_var_arg = true)]
        query: Vec<String>,
    },
    #[command(about = "Find nearest xterm colors for the colors given in hex notation")]
    Xterm {
        #[arg(required = true, trailing_var_arg = true)]
        colors: Vec<String>,
    },
}
