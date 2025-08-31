use anyhow::Result;
use clap::Parser;
use pngsort::{
    config::{ColorChannel, Config, SortMode, SortRange},
    pngsort_main,
};
use std::{
    fs::File,
    io::{BufReader, Write},
};

#[derive(Parser, Debug)]
pub struct Args {
    #[clap(short, long)]
    pub input: String,
    #[clap(short, long)]
    pub output: String,
    #[clap(short, long)]
    pub descending: bool,
    #[clap(long, default_value = "row")]
    pub sort_range: SortRange,
    #[clap(long)]
    pub sort_mode: Option<SortMode>,
    /// Channels to sort by.
    /// For TiedBySum: channels are summed.
    /// For TiedByOrder: channels create a composite key.
    /// For Untied: each channel is sorted independently.
    #[clap(long, value_delimiter = ',', default_value = "")]
    pub sort_channel: Vec<ColorChannel>,
}

impl Args {
    pub fn config(&self) -> Config {
        Config {
            descending: self.descending,
            sort_range: self.sort_range,
            sort_mode: self.sort_mode,
            sort_channel: self.sort_channel.clone(),
        }
    }
}

pub fn main() -> Result<()> {
    let args = Args::parse();
    let input_file = File::open(&args.input)?;
    let mut output_file = File::create(&args.output)?;
    let output_data = pngsort_main(&args.config(), BufReader::new(input_file))?;
    output_file.write_all(&output_data)?;
    Ok(())
}
