//! The Bedrock look's open book, composed once into the 192-pixel area of `crate::book`: the pack's
//! cover (`book_back`) around one of its pages. Bedrock's own screen is two pages wide; the layout
//! here stays Java's.

use image::{RgbaImage, imageops};

const AREA: u32 = 192;
/// The cover and the page on it, as `[x, y, width, height]`; the page is `crate::book`'s.
const COVER: [u32; 4] = [14, 0, 158, 188];
const PAGE: [u32; 4] = [20, 1, 146, 180];
/// Kept unstretched, as left, top, right, bottom: the cover's rim (`book_back.json` slices it in
/// half, which leaves nothing to stretch), and the page's ragged right edge and its ends.
const COVER_BORDER: [u32; 4] = [13; 4];
const PAGE_BORDER: [u32; 4] = [0, 40, 14, 40];

/// `image` at `size`, its `border` kept and the rest stretched.
fn stretched(image: &RgbaImage, [left, top, right, bottom]: [u32; 4], size: [u32; 2]) -> RgbaImage {
    let (width, height) = image.dimensions();
    let cuts = |from: u32, to: u32, low: u32, high: u32| [(0, low, 0, low), (low, from - low - high, low, to - low - high), (from - high, high, to - high, high)];
    let mut out = RgbaImage::new(size[0], size[1]);
    for (sy, sh, dy, dh) in cuts(height, size[1], top, bottom) {
        for (sx, sw, dx, dw) in cuts(width, size[0], left, right) {
            if sw == 0 || sh == 0 {
                continue;
            }
            let part = imageops::crop_imm(image, sx, sy, sw, sh).to_image();
            imageops::replace(&mut out, &imageops::resize(&part, dw, dh, imageops::FilterType::Nearest), i64::from(dx), i64::from(dy));
        }
    }
    out
}

/// `None` without the pack's `book_back` and `book_pageedge_right`.
pub(super) fn art(ui: &dyn Fn(&str) -> Option<RgbaImage>) -> Option<RgbaImage> {
    let (cover, page) = (ui("book_back")?, ui("book_pageedge_right")?);
    let mut art = RgbaImage::new(AREA, AREA);
    imageops::overlay(&mut art, &stretched(&cover, COVER_BORDER, [COVER[2], COVER[3]]), i64::from(COVER[0]), i64::from(COVER[1]));
    imageops::overlay(&mut art, &stretched(&page, PAGE_BORDER, [PAGE[2], PAGE[3]]), i64::from(PAGE[0]), i64::from(PAGE[1]));
    Some(art)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stretched_image_keeps_its_corners() {
        let mut image = RgbaImage::from_pixel(8, 8, image::Rgba([9, 9, 9, 255]));
        image.put_pixel(0, 0, image::Rgba([1, 2, 3, 255]));
        image.put_pixel(7, 7, image::Rgba([4, 5, 6, 255]));
        let out = stretched(&image, [4, 2, 0, 3], [30, 20]);
        assert_eq!((out.dimensions(), out.get_pixel(0, 0).0, out.get_pixel(29, 19).0), ((30, 20), [1, 2, 3, 255], [4, 5, 6, 255]));
        assert_eq!(out.get_pixel(15, 10).0, [9, 9, 9, 255], "the middle is filled");
    }
}
