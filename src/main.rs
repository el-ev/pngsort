use anyhow::Result;
use clap::Parser;
use png::ColorType;
use std::fs::File;
use std::io::BufReader;

type SortFn = Box<dyn Fn(&&[u8]) -> u32>;

#[derive(clap::ValueEnum, Clone, Copy, Debug)]
enum SortRange {
    Row,
    Column,
    RowMajor,
    ColumnMajor,
}

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
    #[clap(long, default_value = "row")]
    sort_range: SortRange,
    #[clap(long)]
    sort_mode: Option<SortMode>,
    /// Channels to sort by.
    /// For TiedBySum: channels are summed.
    /// For TiedByOrder: channels create a composite key.
    /// For Untied: each channel is sorted independently.
    #[clap(long, value_delimiter = ',', default_value = "r,g,b")]
    sort_channel: Vec<ColorChannel>,
}

impl Args {
    fn validate(&self, color_type: ColorType) -> Result<()> {
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

fn main() -> Result<()> {
    let args = Args::parse();
    let input_file = File::open(&args.input)?;
    let output_file = File::create(&args.output)?;
    let reader = BufReader::new(input_file);
    let decoder = png::Decoder::new(reader);
    let mut reader = decoder.read_info()?;
    let info = reader.info();

    let color_type = info.color_type;
    let bit_depth = info.bit_depth;

    args.validate(color_type)?;

    let width = info.width;
    let height = info.height;
    let mut src_buf = vec![0; reader.output_buffer_size().unwrap()];
    reader.next_frame(&mut src_buf)?;

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
        ColorType::Grayscale => 1,
        ColorType::GrayscaleAlpha => 2,
        ColorType::Rgb => 3,
        ColorType::Rgba => 4,
        ColorType::Indexed => unreachable!(),
    };

    let mut out_buf = vec![0; src_buf.len()];

    if args.sort_mode != Some(SortMode::Untied) {
        sort_pixels_tied(
            args,
            src_buf,
            &mut out_buf,
            width,
            height,
            bytes_per_pixel,
            color_type,
        )?;
    } else {
        sort_channels_untied(args, src_buf, &mut out_buf, width, height, bytes_per_pixel)?;
    }

    Ok(out_buf)
}

fn create_sort_function(args: &Args, color_type: ColorType) -> SortFn {
    match color_type {
        ColorType::Grayscale | ColorType::GrayscaleAlpha => Box::new(|pixel| pixel[0] as u32),
        ColorType::Rgb | ColorType::Rgba => {
            let channels = args.sort_channel.clone();
            let mode = args.sort_mode;
            Box::new(move |pixel| {
                let mut key = 0u32;
                for channel in &channels {
                    let idx = channel.index();
                    match mode {
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
    }
}

fn sort_pixels_tied(
    args: &Args,
    src_buf: &[u8],
    out_buf: &mut [u8],
    width: usize,
    height: usize,
    bytes_per_pixel: usize,
    color_type: ColorType,
) -> Result<()> {
    let sort_fn = create_sort_function(args, color_type);

    match args.sort_range {
        SortRange::Row => {
            sort_by_rows(
                src_buf,
                out_buf,
                width,
                height,
                bytes_per_pixel,
                &sort_fn,
                args.descending,
            );
        }
        SortRange::Column => {
            sort_by_columns(
                src_buf,
                out_buf,
                width,
                height,
                bytes_per_pixel,
                &sort_fn,
                args.descending,
            );
        }
        SortRange::RowMajor => {
            sort_row_major(src_buf, out_buf, bytes_per_pixel, &sort_fn, args.descending);
        }
        SortRange::ColumnMajor => {
            sort_column_major(
                src_buf,
                out_buf,
                width,
                height,
                bytes_per_pixel,
                &sort_fn,
                args.descending,
            );
        }
    }

    Ok(())
}

fn sort_by_rows(
    src_buf: &[u8],
    out_buf: &mut [u8],
    width: usize,
    height: usize,
    bytes_per_pixel: usize,
    sort_fn: &dyn Fn(&&[u8]) -> u32,
    descending: bool,
) {
    for y in 0..height {
        let start = y * width * bytes_per_pixel;
        let end = start + width * bytes_per_pixel;
        let mut pixels: Vec<&[u8]> = src_buf[start..end].chunks_exact(bytes_per_pixel).collect();

        pixels.sort_by_key(|p| sort_fn(p));
        if descending {
            pixels.reverse();
        }

        let line = &mut out_buf[start..end];
        for (dst, src_pixel) in line.chunks_exact_mut(bytes_per_pixel).zip(pixels.iter()) {
            dst.copy_from_slice(src_pixel);
        }
    }
}

fn sort_by_columns(
    src_buf: &[u8],
    out_buf: &mut [u8],
    width: usize,
    height: usize,
    bytes_per_pixel: usize,
    sort_fn: &dyn Fn(&&[u8]) -> u32,
    descending: bool,
) {
    for x in 0..width {
        let mut column: Vec<&[u8]> = Vec::with_capacity(height);
        for y in 0..height {
            let idx = (y * width + x) * bytes_per_pixel;
            column.push(&src_buf[idx..idx + bytes_per_pixel]);
        }

        column.sort_by_key(|p| sort_fn(p));
        if descending {
            column.reverse();
        }

        for (y, pixel) in column.iter().enumerate() {
            let idx = (y * width + x) * bytes_per_pixel;
            out_buf[idx..idx + bytes_per_pixel].copy_from_slice(pixel);
        }
    }
}

fn sort_row_major(
    src_buf: &[u8],
    out_buf: &mut [u8],
    bytes_per_pixel: usize,
    sort_fn: &dyn Fn(&&[u8]) -> u32,
    descending: bool,
) {
    let mut pixels: Vec<&[u8]> = src_buf.chunks_exact(bytes_per_pixel).collect();
    pixels.sort_by_key(|p| sort_fn(p));
    if descending {
        pixels.reverse();
    }

    for (dst, src_pixel) in out_buf.chunks_exact_mut(bytes_per_pixel).zip(pixels.iter()) {
        dst.copy_from_slice(src_pixel);
    }
}

fn sort_column_major(
    src_buf: &[u8],
    out_buf: &mut [u8],
    width: usize,
    height: usize,
    bytes_per_pixel: usize,
    sort_fn: &dyn Fn(&&[u8]) -> u32,
    descending: bool,
) {
    let mut pixels: Vec<&[u8]> = src_buf.chunks_exact(bytes_per_pixel).collect();
    pixels.sort_by_key(|p| sort_fn(p));
    if descending {
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

fn sort_channels_untied(
    args: &Args,
    src_buf: &[u8],
    out_buf: &mut [u8],
    width: usize,
    height: usize,
    bytes_per_pixel: usize,
) -> Result<()> {
    out_buf.copy_from_slice(src_buf);

    match args.sort_range {
        SortRange::Row => {
            sort_channels_by_rows(args, out_buf, width, height, bytes_per_pixel);
        }
        SortRange::Column => {
            sort_channels_by_columns(args, out_buf, width, height, bytes_per_pixel);
        }
        SortRange::RowMajor => {
            sort_channels_row_major(args, out_buf, width, height, bytes_per_pixel);
        }
        SortRange::ColumnMajor => {
            sort_channels_column_major(args, out_buf, width, height, bytes_per_pixel);
        }
    }

    Ok(())
}

fn sort_channels_by_rows(
    args: &Args,
    out_buf: &mut [u8],
    width: usize,
    height: usize,
    bytes_per_pixel: usize,
) {
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

            for (x, &value) in channel_buf.iter().enumerate() {
                let idx = (y * width + x) * bytes_per_pixel + channel.index();
                out_buf[idx] = value;
            }
        }
    }
}

fn sort_channels_by_columns(
    args: &Args,
    out_buf: &mut [u8],
    width: usize,
    height: usize,
    bytes_per_pixel: usize,
) {
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

            for (y, &value) in channel_buf.iter().enumerate() {
                let idx = (y * width + x) * bytes_per_pixel + channel.index();
                out_buf[idx] = value;
            }
        }
    }
}

fn sort_channels_row_major(
    args: &Args,
    out_buf: &mut [u8],
    width: usize,
    height: usize,
    bytes_per_pixel: usize,
) {
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

fn sort_channels_column_major(
    args: &Args,
    out_buf: &mut [u8],
    width: usize,
    height: usize,
    bytes_per_pixel: usize,
) {
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
