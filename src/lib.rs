pub mod config;

use std::io::{BufRead, BufReader, Cursor, Seek};

use anyhow::Result;
use config::{Config, SortMode, SortRange};
use png::ColorType;

use wasm_bindgen::prelude::*;

macro_rules! console_log {
    ( $( $t:tt )* ) => {
        web_sys::console::log_1(&format!( $( $t )* ).into());
    }
}

type SortFn = Box<dyn Fn(&&[u8]) -> i32>;

#[wasm_bindgen]
pub fn wasm_main(config: &str, src: &[u8]) -> Result<Vec<u8>, JsValue> {
    let args: Config =
        serde_json::from_str(config).map_err(|e| JsValue::from_str(&e.to_string()))?;
    console_log!("Config: {:?}", args);
    console_log!("Input size: {}", src.len());
    let output_data = pngsort_main(&args, BufReader::new(Cursor::new(src)))
        .map_err(|e| JsValue::from_str(&e.to_string()))?;
    Ok(output_data)
}

pub fn pngsort_main(config: &Config, src: impl BufRead + Seek) -> Result<Vec<u8>> {
    let decoder = png::Decoder::new(src);
    let mut reader = decoder.read_info()?;
    let info = reader.info();

    let color_type = info.color_type;
    let bit_depth = info.bit_depth;

    config.validate(color_type)?;

    let width = info.width;
    let height = info.height;
    let mut src_buf = vec![0; reader.output_buffer_size().unwrap()];
    reader.next_frame(&mut src_buf)?;

    let sorted_buf = process_image(
        config,
        &src_buf,
        width as usize,
        height as usize,
        color_type,
    )?;

    let mut encoded_buf: Vec<u8> = Vec::new();
    let mut encoder = png::Encoder::new(&mut encoded_buf, width, height);
    encoder.set_color(color_type);
    encoder.set_depth(bit_depth);
    let mut writer = encoder.write_header()?;
    writer.write_image_data(&sorted_buf)?;
    writer.finish()?;

    Ok(encoded_buf)
}

fn process_image(
    config: &Config,
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

    if config.sort_mode != Some(SortMode::Untied) {
        sort_pixels_tied(
            config,
            src_buf,
            &mut out_buf,
            width,
            height,
            bytes_per_pixel,
            color_type,
        )?;
    } else {
        sort_channels_untied(
            config,
            src_buf,
            &mut out_buf,
            width,
            height,
            bytes_per_pixel,
        )?;
    }

    Ok(out_buf)
}

fn create_sort_function(config: &Config, color_type: ColorType) -> SortFn {
    let asc: SortFn = match color_type {
        ColorType::Grayscale | ColorType::GrayscaleAlpha => Box::new(|pixel| pixel[0] as i32),
        ColorType::Rgb | ColorType::Rgba => {
            let channels = config.sort_channel.clone();
            let mode = config.sort_mode;
            Box::new(move |pixel| {
                channels.iter().fold(0i32, |key, channel| {
                    let idx = channel.index();
                    match mode {
                        Some(SortMode::TiedBySum) | None => key + pixel[idx] as i32,
                        Some(SortMode::TiedByOrder) => (key << 8) | (pixel[idx] as i32),
                        _ => unreachable!(),
                    }
                })
            })
        }
        _ => unreachable!(),
    };
    if config.descending {
        Box::new(move |pixel| -asc(pixel))
    } else {
        asc
    }
}

fn sort_pixels_tied(
    config: &Config,
    src_buf: &[u8],
    out_buf: &mut [u8],
    width: usize,
    height: usize,
    bytes_per_pixel: usize,
    color_type: ColorType,
) -> Result<()> {
    let sort_fn = create_sort_function(config, color_type);

    match config.sort_range {
        SortRange::Row => {
            sort_by_rows(src_buf, out_buf, width, height, bytes_per_pixel, &sort_fn);
        }
        SortRange::Column => {
            sort_by_columns(src_buf, out_buf, width, height, bytes_per_pixel, &sort_fn);
        }
        SortRange::RowMajor => {
            sort_row_major(src_buf, out_buf, bytes_per_pixel, &sort_fn);
        }
        SortRange::ColumnMajor => {
            sort_column_major(src_buf, out_buf, width, height, bytes_per_pixel, &sort_fn);
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
    sort_fn: &dyn Fn(&&[u8]) -> i32,
) {
    for y in 0..height {
        let start = y * width * bytes_per_pixel;
        let end = start + width * bytes_per_pixel;
        let mut pixels: Vec<&[u8]> = src_buf[start..end].chunks_exact(bytes_per_pixel).collect();

        pixels.sort_by_key(|p| sort_fn(p));
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
    sort_fn: &dyn Fn(&&[u8]) -> i32,
) {
    for x in 0..width {
        let mut column: Vec<&[u8]> = Vec::with_capacity(height);
        for y in 0..height {
            let idx = (y * width + x) * bytes_per_pixel;
            column.push(&src_buf[idx..idx + bytes_per_pixel]);
        }

        column.sort_by_key(|p| sort_fn(p));

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
    sort_fn: &dyn Fn(&&[u8]) -> i32,
) {
    let mut pixels: Vec<&[u8]> = src_buf.chunks_exact(bytes_per_pixel).collect();
    pixels.sort_by_key(|p| sort_fn(p));

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
    sort_fn: &dyn Fn(&&[u8]) -> i32,
) {
    let mut pixels: Vec<&[u8]> = src_buf.chunks_exact(bytes_per_pixel).collect();
    pixels.sort_by_key(|p| sort_fn(p));

    for x in 0..width {
        for y in 0..height {
            let idx = (y * width + x) * bytes_per_pixel;
            let src_pixel = pixels[x * height + y];
            out_buf[idx..idx + bytes_per_pixel].copy_from_slice(src_pixel);
        }
    }
}

fn sort_channels_untied(
    config: &Config,
    src_buf: &[u8],
    out_buf: &mut [u8],
    width: usize,
    height: usize,
    bytes_per_pixel: usize,
) -> Result<()> {
    out_buf.copy_from_slice(src_buf);

    match config.sort_range {
        SortRange::Row => {
            sort_channels_by_rows(config, out_buf, width, height, bytes_per_pixel);
        }
        SortRange::Column => {
            sort_channels_by_columns(config, out_buf, width, height, bytes_per_pixel);
        }
        SortRange::RowMajor => {
            sort_channels_row_major(config, out_buf, width, height, bytes_per_pixel);
        }
        SortRange::ColumnMajor => {
            sort_channels_column_major(config, out_buf, width, height, bytes_per_pixel);
        }
    }

    Ok(())
}

fn sort_channels_by_rows(
    config: &Config,
    out_buf: &mut [u8],
    width: usize,
    height: usize,
    bytes_per_pixel: usize,
) {
    let mut channel_buf: Vec<u8> = Vec::new();
    channel_buf.reserve(width);

    for y in 0..height {
        for channel in &config.sort_channel {
            channel_buf.clear();
            for x in 0..width {
                let idx = (y * width + x) * bytes_per_pixel + channel.index();
                channel_buf.push(out_buf[idx]);
            }

            channel_buf.sort_unstable_by(|a, b| {
                if config.descending {
                    b.cmp(a)
                } else {
                    a.cmp(b)
                }
            });

            for (x, &value) in channel_buf.iter().enumerate() {
                let idx = (y * width + x) * bytes_per_pixel + channel.index();
                out_buf[idx] = value;
            }
        }
    }
}

fn sort_channels_by_columns(
    config: &Config,
    out_buf: &mut [u8],
    width: usize,
    height: usize,
    bytes_per_pixel: usize,
) {
    let mut channel_buf: Vec<u8> = Vec::new();
    channel_buf.reserve(height);

    for x in 0..width {
        for channel in &config.sort_channel {
            channel_buf.clear();
            for y in 0..height {
                let idx = (y * width + x) * bytes_per_pixel + channel.index();
                channel_buf.push(out_buf[idx]);
            }

            channel_buf.sort_unstable_by(|a, b| {
                if config.descending {
                    b.cmp(a)
                } else {
                    a.cmp(b)
                }
            });

            for (y, &value) in channel_buf.iter().enumerate() {
                let idx = (y * width + x) * bytes_per_pixel + channel.index();
                out_buf[idx] = value;
            }
        }
    }
}

fn sort_channels_row_major(
    config: &Config,
    out_buf: &mut [u8],
    width: usize,
    height: usize,
    bytes_per_pixel: usize,
) {
    let mut channel_buf: Vec<u8> = Vec::with_capacity(width * height);

    for channel in &config.sort_channel {
        channel_buf.clear();
        for y in 0..height {
            for x in 0..width {
                let idx = (y * width + x) * bytes_per_pixel + channel.index();
                channel_buf.push(out_buf[idx]);
            }
        }

        channel_buf.sort_unstable_by(|a, b| {
            if config.descending {
                b.cmp(a)
            } else {
                a.cmp(b)
            }
        });

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
    args: &Config,
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

        channel_buf.sort_unstable_by(|a, b| if args.descending { b.cmp(a) } else { a.cmp(b) });

        for x in 0..width {
            for y in 0..height {
                let dst_idx = (y * width + x) * bytes_per_pixel + channel.index();
                let src_idx = x * height + y;
                out_buf[dst_idx] = channel_buf[src_idx];
            }
        }
    }
}
