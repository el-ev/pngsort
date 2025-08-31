use anyhow::Result;
use png::ColorType;
use serde::Deserialize;

#[derive(Deserialize, clap::ValueEnum, Clone, Copy, Debug)]
pub enum SortRange {
    Row,
    Column,
    RowMajor,
    ColumnMajor,
}

#[derive(Deserialize, clap::ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortMode {
    TiedBySum,
    TiedByOrder,
    Untied,
}

#[derive(Deserialize, clap::ValueEnum, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ColorChannel {
    R,
    G,
    B,
}

impl ColorChannel {
    pub const fn index(&self) -> usize {
        match self {
            ColorChannel::R => 0,
            ColorChannel::G => 1,
            ColorChannel::B => 2,
        }
    }
}

#[derive(Deserialize, Debug)]
pub struct Config {
    #[serde(default)]
    pub descending: bool,
    pub sort_range: SortRange,
    pub sort_mode: Option<SortMode>,
    #[serde(default)]
    pub sort_channel: Vec<ColorChannel>,
}

impl Config {
    pub fn validate(&self, color_type: ColorType) -> Result<()> {
        let mut sorted_channels = self.sort_channel.clone();
        sorted_channels.sort();
        sorted_channels.dedup();
        if sorted_channels.len() != self.sort_channel.len() {
            anyhow::bail!("Duplicate channels are not allowed in sort_channel");
        }

        match color_type {
            ColorType::Rgb | ColorType::Rgba => {
                if let Some(SortMode::Untied) = self.sort_mode
                    && self.sort_channel.is_empty()
                {
                    anyhow::bail!("Sort channel should be specified when using Untied sort mode");
                }
            }
            ColorType::Grayscale | ColorType::GrayscaleAlpha => {
                if self.sort_mode.is_some() {
                    anyhow::bail!("Sort mode option is not applicable for Grayscale images");
                }
                if !self.sort_channel.is_empty() {
                    anyhow::bail!("Channel option is not applicable for Grayscale images");
                }
            }
            ColorType::Indexed => anyhow::bail!("Indexed color type is not supported"),
        }

        Ok(())
    }
}
