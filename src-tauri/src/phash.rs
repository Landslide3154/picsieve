use image::imageops::FilterType;
use image::{DynamicImage, GrayImage};
use std::f64::consts::PI;

const SIDE: usize = 32;
const KEEP: usize = 8;

/// 64 位感知哈希（pHash）。
///
/// 步骤：转灰度 → 缩到 32×32 → 二维 DCT-II → 取左上 8×8 → 与中位数比较成位。
/// 对缩放、重压缩稳定；对内容不同的图区分度足够。
pub fn phash_u64(img: &DynamicImage) -> u64 {
    let small: GrayImage = img
        .resize_exact(SIDE as u32, SIDE as u32, FilterType::Lanczos3)
        .to_luma8();

    // 拉成 f64 矩阵
    let mut m = vec![vec![0f64; SIDE]; SIDE];
    for (x, y, p) in small.enumerate_pixels() {
        m[y as usize][x as usize] = p.0[0] as f64;
    }

    // 可分离 DCT-II：先对每行，再对每列
    let cos_table = cos_table();
    let rows: Vec<Vec<f64>> = m.iter().map(|row| dct_1d(row, &cos_table)).collect();
    let mut cols = vec![vec![0f64; SIDE]; SIDE];
    for x in 0..SIDE {
        let col: Vec<f64> = (0..SIDE).map(|y| rows[y][x]).collect();
        let out = dct_1d(&col, &cos_table);
        for (dst, v) in cols.iter_mut().zip(out.iter()) {
            dst[x] = *v;
        }
    }

    // 取左上 8×8，跳过 DC 分量
    let mut vals = Vec::with_capacity(KEEP * KEEP - 1);
    for (y, row) in cols.iter().enumerate().take(KEEP) {
        for (x, v) in row.iter().enumerate().take(KEEP) {
            if x == 0 && y == 0 {
                continue;
            }
            vals.push(*v);
        }
    }
    let median = median_of(&mut vals);

    let mut bits = 0u64;
    let mut idx = 0;
    for (y, row) in cols.iter().enumerate().take(KEEP) {
        for (x, v) in row.iter().enumerate().take(KEEP) {
            if x == 0 && y == 0 {
                continue;
            }
            if *v > median {
                bits |= 1u64 << idx;
            }
            idx += 1;
        }
    }
    bits
}

fn cos_table() -> Vec<Vec<f64>> {
    (0..SIDE)
        .map(|k| {
            (0..SIDE)
                .map(|n| ((2.0 * n as f64 + 1.0) * k as f64 * PI / (2.0 * SIDE as f64)).cos())
                .collect()
        })
        .collect()
}

fn dct_1d(input: &[f64], cos_table: &[Vec<f64>]) -> Vec<f64> {
    (0..SIDE)
        .map(|k| {
            let s: f64 = (0..SIDE).map(|n| input[n] * cos_table[k][n]).sum();
            let scale = if k == 0 {
                (1.0 / SIDE as f64).sqrt()
            } else {
                (2.0 / SIDE as f64).sqrt()
            };
            s * scale
        })
        .collect()
}

fn median_of(vals: &mut [f64]) -> f64 {
    vals.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = vals.len();
    if n == 0 {
        0.0
    } else if n % 2 == 1 {
        vals[n / 2]
    } else {
        (vals[n / 2 - 1] + vals[n / 2]) / 2.0
    }
}

pub fn hamming(a: u64, b: u64) -> u32 {
    (a ^ b).count_ones()
}

pub fn to_hex(v: u64) -> String {
    format!("{v:016x}")
}

pub fn from_hex(s: &str) -> Option<u64> {
    u64::from_str_radix(s, 16).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgb, RgbImage};

    /// 构造一张有渐变和色块的图，作为「特征明显的原图」
    fn busy_image(w: u32, h: u32, seed: u8) -> RgbImage {
        let mut img = RgbImage::new(w, h);
        for (x, y, p) in img.enumerate_pixels_mut() {
            let r = ((x * 255 / w.max(1)) as u8).wrapping_add(seed);
            let g = (y * 255 / h.max(1)) as u8;
            let b = if (x / 8 + y / 8) % 2 == 0 { 40 } else { 200 };
            *p = Rgb([r, g, b]);
        }
        img
    }

    #[test]
    fn same_image_has_zero_distance() {
        let img = busy_image(300, 400, 0);
        let a = phash_u64(&image::DynamicImage::ImageRgb8(img.clone()));
        let b = phash_u64(&image::DynamicImage::ImageRgb8(img));
        assert_eq!(hamming(a, b), 0);
    }

    #[test]
    fn scaled_copy_stays_close() {
        let img = busy_image(300, 400, 0);
        let small = image::DynamicImage::ImageRgb8(img.clone()).resize_exact(
            120,
            160,
            image::imageops::FilterType::Lanczos3,
        );
        let a = phash_u64(&image::DynamicImage::ImageRgb8(img));
        let b = phash_u64(&small);
        assert!(
            hamming(a, b) <= 8,
            "缩放后汉明距离应仍很小，实际 {}",
            hamming(a, b)
        );
    }

    #[test]
    fn different_images_are_far_apart() {
        let a = phash_u64(&image::DynamicImage::ImageRgb8(busy_image(300, 400, 0)));
        let b = phash_u64(&image::DynamicImage::ImageRgb8(busy_image(300, 400, 128)));
        assert!(hamming(a, b) > 4, "不同图不应过近，实际 {}", hamming(a, b));
    }

    #[test]
    fn hex_roundtrip() {
        let v = 0x0123_4567_89AB_CDEFu64;
        assert_eq!(from_hex(&to_hex(v)), Some(v));
        assert_eq!(from_hex("zzz"), None);
    }
}
