use anyhow::Result;
use clap::Parser;
use png::ColorType;
use std::fs::File;
use std::io::BufReader;

#[derive(clap::ValueEnum, Clone, Copy, Debug)]
enum SortRange {
    Row,
    Column,
    RowMajor,
    ColumnMajor,
}

// For RGB/RGBA images
#[derive(clap::ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
enum SortMode {
    TiedBySum,
    TiedByOrder,
    Untied,
}

#[derive(clap::ValueEnum, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum ColorChannel {
    R,
    G,
    B,
}

impl ColorChannel {
    fn index(&self) -> usize {
        match self {
            ColorChannel::R => 0,
            ColorChannel::G => 1,
            ColorChannel::B => 2,
        }
    }
}

#[derive(Parser, Debug)]
struct Args {
    #[clap(short, long)]
    input: String,
    #[clap(short, long)]
    output: String,
    #[clap(long)]
    descending: bool,
    #[clap(long, default_value = "line")]
    sort_range: SortRange,
    #[clap(long)]
    sort_mode: Option<SortMode>,
    // works as sum, or order, or affected channels depending on sort_mode
    #[clap(long, value_delimiter = ',', default_value = "r,g,b")]
    sort_channel: Vec<ColorChannel>,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let input_file = File::open(&args.input)?;
    let output_file = File::create(&args.output)?;
    let reader = BufReader::new(input_file);
    let decoder = png::Decoder::new(reader);
    let mut reader = decoder.read_info()?;
    let info = reader.info();

    {
        let mut sorted_channels = args.sort_channel.clone();
        sorted_channels.sort();
        sorted_channels.dedup();
        if sorted_channels.len() != args.sort_channel.len() {
            anyhow::bail!("Duplicate channels are not allowed in sort_channel");
        }
    }

    let color_type = info.color_type;
    let bit_depth = info.bit_depth;

    match color_type {
        ColorType::Rgb | ColorType::Rgba => {
            if let Some(SortMode::Untied) = args.sort_mode
                && args.sort_channel.is_empty()
            {
                anyhow::bail!("Sort channel should be specified when using Untied sort mode");
            }
        }
        ColorType::Grayscale | ColorType::GrayscaleAlpha => {
            if args.sort_mode.is_some() {
                anyhow::bail!("Sort mode option is not applicable for Grayscale images");
            }
            if !args.sort_channel.is_empty() {
                anyhow::bail!("Channel option is not applicable for Grayscale images");
            }
        }
        ColorType::Indexed => anyhow::bail!("Indexed color type is not supported"),
    }

    let width = info.width;
    let height = info.height;
    let mut src_buf = vec![0; reader.output_buffer_size().unwrap()];
    while reader.next_frame(&mut src_buf).is_ok() {}

    let sorted_buf = process_image(&args, &src_buf, width as usize, height as usize, color_type)?;

    let mut encoder = png::Encoder::new(output_file, width, height);
    encoder.set_color(color_type);
    encoder.set_depth(bit_depth);
    let mut writer = encoder.write_header()?;
    writer.write_image_data(&sorted_buf)?;
    writer.finish()?;

    Ok(())
}

fn process_image(
    args: &Args,
    src_buf: &[u8],
    width: usize,
    height: usize,
    color_type: ColorType,
) -> Result<Vec<u8>> {
    let bytes_per_pixel = match color_type {
        ColorType::Rgb => 3,
        ColorType::Rgba => 4,
        ColorType::Grayscale => 1,
        ColorType::GrayscaleAlpha => 2,
        ColorType::Indexed => unreachable!(),
    };
    let mut out_buf = vec![0; src_buf.len()];
    if args.sort_mode != Some(SortMode::Untied) {
        let sort_fn: Box<dyn Fn(&&[u8]) -> u32> = match color_type {
            ColorType::Grayscale | ColorType::GrayscaleAlpha => Box::new(|pixel| pixel[0] as u32),
            ColorType::Rgb | ColorType::Rgba => {
                Box::new(|pixel| {
                    let mut key = 0u32;
                    for channel in &args.sort_channel {
                        let idx = channel.index();
                        match args.sort_mode {
                            Some(SortMode::TiedBySum) | None => {
                                key += pixel[idx] as u32;
                            }
                            Some(SortMode::TiedByOrder) => {
                                key = (key << 8) | (pixel[idx] as u32);
                            }
                            _ => unreachable!(),
                        }
                    }
                    key
                })
            }
            _ => unreachable!(),
        };
        match args.sort_range {
            SortRange::Row => {
                for y in 0..height {
                    let start = y * width * bytes_per_pixel;
                    let end = start + width * bytes_per_pixel;
                    let mut pixels: Vec<&[u8]> =
                        src_buf[start..end].chunks_exact(bytes_per_pixel).collect();
                    pixels.sort_by_key(|p| sort_fn(p));
                    if args.descending {
                        pixels.reverse();
                    }
                    let line = &mut out_buf[start..end];
                    for (dst, src_pixel) in
                        line.chunks_exact_mut(bytes_per_pixel).zip(pixels.iter())
                    {
                        dst.copy_from_slice(src_pixel);
                    }
                }
            }
            SortRange::Column => {
                for x in 0..width {
                    let mut column: Vec<&[u8]> = Vec::with_capacity(height);
                    for y in 0..height {
                        let idx = (y * width + x) * bytes_per_pixel;
                        column.push(&src_buf[idx..idx + bytes_per_pixel]);
                    }
                    column.sort_by_key(|p| sort_fn(p));
                    if args.descending {
                        column.reverse();
                    }
                    for (y, pixel) in column.iter().enumerate() {
                        let idx = (y * width + x) * bytes_per_pixel;
                        out_buf[idx..idx + bytes_per_pixel].copy_from_slice(pixel);
                    }
                }
            }
            SortRange::RowMajor => {
                let mut pixels: Vec<&[u8]> = src_buf.chunks_exact(bytes_per_pixel).collect();
                pixels.sort_by_key(|p| sort_fn(p));
                if args.descending {
                    pixels.reverse();
                }
                for (dst, src_pixel) in out_buf.chunks_exact_mut(bytes_per_pixel).zip(pixels.iter())
                {
                    dst.copy_from_slice(src_pixel);
                }
            }
            SortRange::ColumnMajor => {
                let mut pixels: Vec<&[u8]> = src_buf.chunks_exact(bytes_per_pixel).collect();
                pixels.sort_by_key(|p| sort_fn(p));
                if args.descending {
                    pixels.reverse();
                }
                for x in 0..width {
                    for y in 0..height {
                        let idx = (y * width + x) * bytes_per_pixel;
                        let src_pixel = pixels[x * height + y];
                        out_buf[idx..idx + bytes_per_pixel].copy_from_slice(src_pixel);
                    }
                }
            }
        };
    } else {
        out_buf.copy_from_slice(src_buf);
        match args.sort_range {
            SortRange::Row => {
                let mut channel_buf: Vec<u8> = Vec::new();
                channel_buf.reserve(width);
                for y in 0..height {
                    for channel in &args.sort_channel {
                        channel_buf.clear();
                        for x in 0..width {
                            let idx = (y * width + x) * bytes_per_pixel + channel.index();
                            channel_buf.push(out_buf[idx]);
                        }
                        channel_buf.sort_unstable();
                        if args.descending {
                            channel_buf.reverse();
                        }
                        for (x, &b) in channel_buf.iter().enumerate().take(height) {
                            let idx = (y * width + x) * bytes_per_pixel + channel.index();
                            out_buf[idx] = b;
                        }
                    }
                }
            }
            SortRange::Column => {
                let mut channel_buf: Vec<u8> = Vec::new();
                channel_buf.reserve(height);
                for x in 0..width {
                    for channel in &args.sort_channel {
                        channel_buf.clear();
                        for y in 0..height {
                            let idx = (y * width + x) * bytes_per_pixel + channel.index();
                            channel_buf.push(out_buf[idx]);
                        }
                        channel_buf.sort_unstable();
                        if args.descending {
                            channel_buf.reverse();
                        }
                        for (y, &b) in channel_buf.iter().enumerate().take(height) {
                            let idx = (y * width + x) * bytes_per_pixel + channel.index();
                            out_buf[idx] = b;
                        }
                    }
                }
            }
            SortRange::RowMajor => {
                let mut channel_buf: Vec<u8> = Vec::with_capacity(width * height);
                for channel in &args.sort_channel {
                    channel_buf.clear();
                    for y in 0..height {
                        for x in 0..width {
                            let idx = (y * width + x) * bytes_per_pixel + channel.index();
                            channel_buf.push(out_buf[idx]);
                        }
                    }
                    channel_buf.sort_unstable();
                    if args.descending {
                        channel_buf.reverse();
                    }
                    let mut i = 0;
                    for y in 0..height {
                        for x in 0..width {
                            let idx = (y * width + x) * bytes_per_pixel + channel.index();
                            out_buf[idx] = channel_buf[i];
                            i += 1;
                        }
                    }
                }
            }
            SortRange::ColumnMajor => {
                let mut channel_buf: Vec<u8> = Vec::with_capacity(width * height);
                for channel in &args.sort_channel {
                    channel_buf.clear();
                    for y in 0..height {
                        for x in 0..width {
                            let idx = (y * width + x) * bytes_per_pixel + channel.index();
                            channel_buf.push(out_buf[idx]);
                        }
                    }
                    channel_buf.sort_unstable();
                    if args.descending {
                        channel_buf.reverse();
                    }
                    for x in 0..width {
                        for y in 0..height {
                            let dst_idx = (y * width + x) * bytes_per_pixel + channel.index();
                            let src_idx = x * height + y;
                            out_buf[dst_idx] = channel_buf[src_idx];
                        }
                    }
                }
            }
        }
    }
    Ok(out_buf)
}
