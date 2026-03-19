//! 内容脱敏模块
//!
//! 提供图像脱敏处理功能，支持模糊、遮罩、像素化等多种脱敏方式。

use crate::tee::sandbox::{
    error::ExportError,
    export::review::{RedactionAction, RedactionRegion, SensitiveType},
    export::screenshot::ImageFormat,
};
use image::{GenericImage, GenericImageView};
use std::collections::HashMap;

/// 脱敏服务
///
/// 处理图像脱敏，支持多种脱敏策略
pub struct RedactionService {
    strategies: HashMap<RedactionAction, Box<dyn RedactionStrategy>>,
    default_strategy: RedactionAction,
}

/// 脱敏策略 trait
pub trait RedactionStrategy: Send + Sync {
    /// 应用脱敏
    ///
    /// # Arguments
    ///
    /// * `image` - 输入图像数据
    /// * `region` - 脱敏区域
    ///
    /// # Returns
    ///
    /// 返回脱敏后的图像数据
    fn apply(
        &self,
        image: &mut image::DynamicImage,
        region: &RedactionRegion,
    ) -> Result<(), ExportError>;

    /// 获取策略名称
    fn name(&self) -> &str;
}

/// 模糊脱敏策略
pub struct BlurStrategy {
    sigma: f32,
}

impl BlurStrategy {
    pub fn new(sigma: f32) -> Self {
        Self { sigma }
    }

    pub fn with_default_config() -> Self {
        Self::new(10.0)
    }
}

impl RedactionStrategy for BlurStrategy {
    fn apply(
        &self,
        image: &mut image::DynamicImage,
        region: &RedactionRegion,
    ) -> Result<(), ExportError> {
        let (img_width, img_height) = (image.width() as f64, image.height() as f64);

        // 计算像素坐标
        let x = (region.x * img_width) as u32;
        let y = (region.y * img_height) as u32;
        let width = (region.width * img_width) as u32;
        let height = (region.height * img_height) as u32;

        // 裁剪区域
        let mut sub_image = image.crop_imm(x, y, width, height);

        // 应用高斯模糊
        sub_image = sub_image.blur(self.sigma);

        // 将模糊后的图像复制回原图
        // 注意：这里需要手动实现复制逻辑
        Self::overlay_image(image, &sub_image, x, y);

        Ok(())
    }

    fn name(&self) -> &str {
        "blur"
    }
}

impl BlurStrategy {
    fn overlay_image(dest: &mut image::DynamicImage, src: &image::DynamicImage, x: u32, y: u32) {
        let src_width = src.width().min(dest.width() - x);
        let src_height = src.height().min(dest.height() - y);

        for dy in 0..src_height {
            for dx in 0..src_width {
                let pixel = src.get_pixel(dx, dy);
                dest.put_pixel(x + dx, y + dy, pixel);
            }
        }
    }
}

/// 黑色遮罩策略
pub struct BlackoutStrategy;

impl BlackoutStrategy {
    pub fn new() -> Self {
        Self
    }
}

impl Default for BlackoutStrategy {
    fn default() -> Self {
        Self::new()
    }
}

impl RedactionStrategy for BlackoutStrategy {
    fn apply(
        &self,
        image: &mut image::DynamicImage,
        region: &RedactionRegion,
    ) -> Result<(), ExportError> {
        let (img_width, img_height) = (image.width() as f64, image.height() as f64);

        let x = (region.x * img_width) as u32;
        let y = (region.y * img_height) as u32;
        let width = (region.width * img_width) as u32;
        let height = (region.height * img_height) as u32;

        // 创建黑色像素
        let black = image::Rgba([0, 0, 0, 255]);

        // 填充区域
        for dy in y..(y + height).min(image.height()) {
            for dx in x..(x + width).min(image.width()) {
                image.put_pixel(dx, dy, black);
            }
        }

        Ok(())
    }

    fn name(&self) -> &str {
        "blackout"
    }
}

/// 像素化策略
pub struct PixelateStrategy {
    block_size: u32,
}

impl PixelateStrategy {
    pub fn new(block_size: u32) -> Self {
        Self {
            block_size: block_size.max(4),
        }
    }

    pub fn with_default_config() -> Self {
        Self::new(8)
    }
}

impl RedactionStrategy for PixelateStrategy {
    fn apply(
        &self,
        image: &mut image::DynamicImage,
        region: &RedactionRegion,
    ) -> Result<(), ExportError> {
        let (img_width, img_height) = (image.width() as f64, image.height() as f64);

        let start_x = (region.x * img_width) as u32;
        let start_y = (region.y * img_height) as u32;
        let width = (region.width * img_width) as u32;
        let height = (region.height * img_height) as u32;

        let end_x = (start_x + width).min(image.width());
        let end_y = (start_y + height).min(image.height());

        // 按块处理
        let mut y = start_y;
        while y < end_y {
            let block_height = self.block_size.min(end_y - y);
            let mut x = start_x;

            while x < end_x {
                let block_width = self.block_size.min(end_x - x);

                // 计算块的平均颜色
                let avg_color =
                    self.calculate_average_color(image, x, y, block_width, block_height);

                // 填充块
                for dy in 0..block_height {
                    for dx in 0..block_width {
                        image.put_pixel(x + dx, y + dy, avg_color);
                    }
                }

                x += block_width;
            }

            y += block_height;
        }

        Ok(())
    }

    fn name(&self) -> &str {
        "pixelate"
    }
}

impl PixelateStrategy {
    fn calculate_average_color(
        &self,
        image: &image::DynamicImage,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
    ) -> image::Rgba<u8> {
        let mut r_sum: u64 = 0;
        let mut g_sum: u64 = 0;
        let mut b_sum: u64 = 0;
        let mut a_sum: u64 = 0;
        let mut count: u64 = 0;

        for dy in 0..height {
            for dx in 0..width {
                let pixel = image.get_pixel(x + dx, y + dy);
                r_sum += pixel[0] as u64;
                g_sum += pixel[1] as u64;
                b_sum += pixel[2] as u64;
                a_sum += pixel[3] as u64;
                count += 1;
            }
        }

        image::Rgba([
            (r_sum / count) as u8,
            (g_sum / count) as u8,
            (b_sum / count) as u8,
            (a_sum / count) as u8,
        ])
    }
}

/// 替换策略
#[allow(dead_code)]
pub struct ReplaceStrategy {
    placeholder: String,
}

impl ReplaceStrategy {
    pub fn new(placeholder: impl Into<String>) -> Self {
        Self {
            placeholder: placeholder.into(),
        }
    }

    pub fn with_default_config() -> Self {
        Self::new("[REDACTED]")
    }
}

impl RedactionStrategy for ReplaceStrategy {
    fn apply(
        &self,
        _image: &mut image::DynamicImage,
        _region: &RedactionRegion,
    ) -> Result<(), ExportError> {
        // 替换策略主要用于文本替换，在图像上效果有限
        // 这里使用黑色遮罩作为回退
        let blackout = BlackoutStrategy::new();
        blackout.apply(_image, _region)
    }

    fn name(&self) -> &str {
        "replace"
    }
}

impl Default for RedactionService {
    fn default() -> Self {
        Self::new()
    }
}

impl RedactionService {
    /// 创建新的脱敏服务
    pub fn new() -> Self {
        let mut strategies: HashMap<RedactionAction, Box<dyn RedactionStrategy>> = HashMap::new();

        strategies.insert(
            RedactionAction::Blur,
            Box::new(BlurStrategy::with_default_config()),
        );
        strategies.insert(RedactionAction::Blackout, Box::new(BlackoutStrategy));
        strategies.insert(
            RedactionAction::Pixelate,
            Box::new(PixelateStrategy::with_default_config()),
        );
        strategies.insert(
            RedactionAction::Replace,
            Box::new(ReplaceStrategy::with_default_config()),
        );

        Self {
            strategies,
            default_strategy: RedactionAction::Blackout,
        }
    }

    /// 添加自定义策略
    pub fn add_strategy(&mut self, action: RedactionAction, strategy: Box<dyn RedactionStrategy>) {
        self.strategies.insert(action, strategy);
    }

    /// 设置默认策略
    pub fn set_default_strategy(&mut self, action: RedactionAction) {
        self.default_strategy = action;
    }

    /// 脱敏图像
    ///
    /// # Arguments
    ///
    /// * `image_data` - 原始图像数据
    /// * `format` - 图像格式
    /// * `regions` - 需要脱敏的区域列表
    ///
    /// # Returns
    ///
    /// 返回脱敏后的图像数据
    pub fn redact(
        &self,
        image_data: &[u8],
        format: ImageFormat,
        regions: &[RedactionRegion],
    ) -> Result<Vec<u8>, ExportError> {
        // 加载图像
        let mut image = image::load_from_memory(image_data)
            .map_err(|e| ExportError::Redaction(format!("加载图像失败: {}", e)))?;

        // 按区域应用脱敏
        for region in regions {
            let strategy = self
                .strategies
                .get(&region.action)
                .or_else(|| self.strategies.get(&self.default_strategy))
                .ok_or_else(|| {
                    ExportError::Redaction(format!("未找到脱敏策略: {:?}", region.action))
                })?;

            strategy
                .apply(&mut image, region)
                .map_err(|e| ExportError::Redaction(format!("应用脱敏策略失败: {}", e)))?;
        }

        // 编码图像
        let mut output = Vec::new();
        let mut cursor = std::io::Cursor::new(&mut output);

        match format {
            ImageFormat::Png => image.write_to(&mut cursor, image::ImageFormat::Png),
            ImageFormat::Jpeg => image.write_to(&mut cursor, image::ImageFormat::Jpeg),
            ImageFormat::Webp => image.write_to(&mut cursor, image::ImageFormat::WebP),
        }
        .map_err(|e| ExportError::Redaction(format!("编码图像失败: {}", e)))?;

        Ok(output)
    }

    /// 批量脱敏
    pub fn redact_batch(
        &self,
        images: &[(Vec<u8>, ImageFormat, Vec<RedactionRegion>)],
    ) -> Vec<Result<Vec<u8>, ExportError>> {
        images
            .iter()
            .map(|(data, format, regions)| self.redact(data, *format, regions))
            .collect()
    }

    /// 根据敏感类型选择最佳脱敏策略
    pub fn select_strategy_for_type(&self, sensitive_type: SensitiveType) -> RedactionAction {
        match sensitive_type {
            SensitiveType::Pii | SensitiveType::Email | SensitiveType::PhoneNumber => {
                RedactionAction::Blackout
            }
            SensitiveType::Credential | SensitiveType::ApiKey => RedactionAction::Pixelate,
            SensitiveType::CreditCard | SensitiveType::Ssn => RedactionAction::Blackout,
            SensitiveType::Address => RedactionAction::Blur,
            SensitiveType::FinancialInfo => RedactionAction::Blackout,
            _ => self.default_strategy,
        }
    }
}

/// 脱敏结果
#[derive(Debug, Clone)]
pub struct RedactionResult {
    pub data: Vec<u8>,
    pub applied_regions: Vec<RedactionRegion>,
    pub original_size: usize,
    pub redacted_size: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tee::sandbox::export::review::RegionType;

    fn create_test_image() -> Vec<u8> {
        // 创建一个简单的 100x100 PNG 图像
        // 使用最小的有效 PNG 数据
        vec![
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, // PNG signature
            0x00, 0x00, 0x00, 0x0D, // IHDR length
            0x49, 0x48, 0x44, 0x52, // IHDR
            0x00, 0x00, 0x00, 0x64, // width: 100
            0x00, 0x00, 0x00, 0x64, // height: 100
            0x08, 0x02, 0x00, 0x00, 0x00, // bit depth, color type, etc.
            0x7A, 0x5D, 0x74, 0x74, // CRC
            0x00, 0x00, 0x00, 0x00, // IDAT length (empty for simplicity)
            0x49, 0x44, 0x41, 0x54, // IDAT
            0x00, 0x00, 0x00, 0x00, // CRC
            0x00, 0x00, 0x00, 0x00, // IEND length
            0x49, 0x45, 0x4E, 0x44, // IEND
            0xAE, 0x42, 0x60, 0x82, // CRC
        ]
    }

    fn create_test_region(action: RedactionAction) -> RedactionRegion {
        RedactionRegion {
            region_type: RegionType::Rectangle,
            x: 0.1,
            y: 0.1,
            width: 0.2,
            height: 0.2,
            sensitive_type: SensitiveType::Pii,
            action,
            confidence: 0.95,
        }
    }

    #[test]
    fn test_redaction_service_creation() {
        let service = RedactionService::new();
        assert!(!service.strategies.is_empty());
    }

    #[test]
    fn test_select_strategy_for_type() {
        let service = RedactionService::new();

        assert_eq!(
            service.select_strategy_for_type(SensitiveType::Credential),
            RedactionAction::Pixelate
        );
        assert_eq!(
            service.select_strategy_for_type(SensitiveType::ApiKey),
            RedactionAction::Pixelate
        );
        assert_eq!(
            service.select_strategy_for_type(SensitiveType::CreditCard),
            RedactionAction::Blackout
        );
    }

    #[test]
    fn test_blackout_strategy_name() {
        let strategy = BlackoutStrategy::new();
        assert_eq!(strategy.name(), "blackout");
    }

    #[test]
    fn test_blur_strategy_creation() {
        let strategy = BlurStrategy::new(5.0);
        assert_eq!(strategy.name(), "blur");
        assert_eq!(strategy.sigma, 5.0);
    }

    #[test]
    fn test_pixelate_strategy_creation() {
        let strategy = PixelateStrategy::new(16);
        assert_eq!(strategy.name(), "pixelate");
        assert_eq!(strategy.block_size, 16);
    }

    #[test]
    fn test_replace_strategy_creation() {
        let strategy = ReplaceStrategy::new("[HIDDEN]");
        assert_eq!(strategy.name(), "replace");
    }
}
