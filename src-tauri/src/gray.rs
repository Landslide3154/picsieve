use image::imageops::FilterType;
use image::DynamicImage;

const SIDE: usize = 64;

/// 灰度分数：先把图缩到 64×64 的 RGB，再逐像素取三通道两两差值中的最大值，
/// 最后取 95 分位。0 表示完全是黑白灰；数值越大越像彩色图。
pub fn gray_score(img: &DynamicImage) -> f64 {
    let small = img
        .resize_exact(SIDE as u32, SIDE as u32, FilterType::Lanczos3)
        .to_rgb8();
    let mut diffs: Vec<f64> = Vec::with_capacity(SIDE * SIDE);
    for (_x, _y, p) in small.enumerate_pixels() {
        let r = p.0[0] as i32;
        let g = p.0[1] as i32;
        let b = p.0[2] as i32;
        let d = (r - g).abs().max((g - b).abs()).max((r - b).abs());
        diffs.push(d as f64);
    }
    percentile(&mut diffs, 95.0)
}

fn percentile(vals: &mut [f64], p: f64) -> f64 {
    if vals.is_empty() {
        return 0.0;
    }
    vals.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let idx = ((p / 100.0) * (vals.len() as f64 - 1.0)).round() as usize;
    vals[idx.min(vals.len() - 1)]
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{DynamicImage, Rgb, RgbImage};

    fn solid(w: u32, h: u32, c: [u8; 3]) -> DynamicImage {
        DynamicImage::ImageRgb8(RgbImage::from_pixel(w, h, Rgb(c)))
    }

    #[test]
    fn pure_gray_scores_zero() {
        assert_eq!(gray_score(&solid(64, 64, [128, 128, 128])), 0.0);
    }

    #[test]
    fn black_and_white_gray_scores_zero() {
        let mut img = RgbImage::new(64, 64);
        for (x, _y, p) in img.enumerate_pixels_mut() {
            let v = if x < 32 { 0 } else { 255 };
            *p = Rgb([v, v, v]);
        }
        assert_eq!(gray_score(&DynamicImage::ImageRgb8(img)), 0.0);
    }

    #[test]
    fn saturated_color_scores_high() {
        assert!(gray_score(&solid(64, 64, [255, 0, 0])) > 100.0);
    }

    #[test]
    fn slightly_tinted_gray_scores_low() {
        assert!(gray_score(&solid(64, 64, [130, 128, 126])) <= 8.0);
    }
}
