pub const DISPLAY_WIDTH: usize = 64;
pub const DISPLAY_HEIGHT: usize = 32;
pub const DISPLAY_PIXELS: usize = DISPLAY_WIDTH * DISPLAY_HEIGHT;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Framebuffer {
    pixels: [u8; DISPLAY_PIXELS],
}

impl Default for Framebuffer {
    fn default() -> Self {
        Self {
            pixels: [0; DISPLAY_PIXELS],
        }
    }
}

impl Framebuffer {
    pub(crate) fn clear(&mut self) {
        self.pixels.fill(0);
    }

    pub(crate) fn pixels(&self) -> &[u8; DISPLAY_PIXELS] {
        &self.pixels
    }

    /// XOR a sprite at `(x, y)`, returning whether an illuminated pixel was erased.
    pub(crate) fn draw(&mut self, x: u8, y: u8, sprite: &[u8], wrap: bool) -> bool {
        let mut collision = false;
        for (row, byte) in sprite.iter().copied().enumerate() {
            let py = usize::from(y) + row;
            if !wrap && py >= DISPLAY_HEIGHT {
                continue;
            }
            let py = py % DISPLAY_HEIGHT;
            for bit in 0..8 {
                if byte & (0x80 >> bit) == 0 {
                    continue;
                }
                let px = usize::from(x) + bit;
                if !wrap && px >= DISPLAY_WIDTH {
                    continue;
                }
                let index = py * DISPLAY_WIDTH + (px % DISPLAY_WIDTH);
                if self.pixels[index] == 1 {
                    collision = true;
                }
                self.pixels[index] ^= 1;
            }
        }
        collision
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xor_drawing_reports_collision() {
        let mut display = Framebuffer::default();
        assert!(!display.draw(0, 0, &[0b1000_0000], true));
        assert_eq!(display.pixels()[0], 1);
        assert!(display.draw(0, 0, &[0b1000_0000], true));
        assert_eq!(display.pixels()[0], 0);
    }

    #[test]
    fn clipped_drawing_does_not_wrap() {
        let mut display = Framebuffer::default();
        display.draw(63, 31, &[0b1100_0000, 0b1000_0000], false);
        assert_eq!(display.pixels()[DISPLAY_PIXELS - 1], 1);
        assert_eq!(display.pixels()[31 * DISPLAY_WIDTH], 0);
        assert_eq!(display.pixels()[0], 0);
    }
}
