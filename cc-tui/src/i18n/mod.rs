use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use fluent::FluentResource;
// 使用 concurrent（基于 Mutex 的 IntlLangMemoizer）变体，使 `LcRegistry` 满足 `Sync`，
// 从而可安全存放于进程级全局注册表并跨线程（如渲染线程）读取。
use fluent_bundle::concurrent::FluentBundle;
use fluent_bundle::{FluentArgs, FluentValue};

const EN_FTL: &str = include_str!("../../locales/en/main.ftl");
const ZH_CN_FTL: &str = include_str!("../../locales/zh-CN/main.ftl");

/// 进程级语言注册表。
///
/// 供**没有 `App`/`ServiceRegistry` 上下文**的静态构造路径（如
/// `MessageViewModel::user/system/from_base_message*` 内部把后台 shell 通知
/// 翻译成可读提示）读取当前语言。启动时由 `App::new` 用真实配置初始化；
/// `/lang` 切换时同步更新。
///
/// 未初始化时回退到默认（`en`），保证测试与早期调用不 panic。
static GLOBAL: RwLock<Option<Arc<LcRegistry>>> = RwLock::new(None);

/// 初始化/覆盖进程级语言注册表（启动与 `/lang` 切换时调用）。
pub fn init_global(lc: LcRegistry) {
    let lc = Arc::new(lc);
    match GLOBAL.write() {
        Ok(mut guard) => *guard = Some(lc),
        Err(poisoned) => *poisoned.into_inner() = Some(lc),
    }
}

/// 读取进程级语言注册表的稳定句柄；未初始化时回退默认（`en`）。
///
/// 返回 `Arc` 克隆（而非深拷贝）：`FluentBundle` 不实现 `Clone`，且多语言
/// bundle 只在首次构造时解析一次，后续切换语言仅需换 `current_lang`。
pub fn global() -> Arc<LcRegistry> {
    let guard = match GLOBAL.read() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    match guard.as_ref() {
        Some(lc) => Arc::clone(lc),
        None => Arc::new(LcRegistry::default()),
    }
}

pub struct LcRegistry {
    current_lang: String,
    bundles: HashMap<String, FluentBundle<FluentResource>>,
}

impl Clone for LcRegistry {
    fn clone(&self) -> Self {
        // 重建 bundle（共享同一份 include_str! FTL 源码；解析成本一次性且可接受）。
        Self::new(Some(&self.current_lang))
    }
}

impl LcRegistry {
    pub fn new(lang: Option<&str>) -> Self {
        let mut bundles = HashMap::new();
        bundles.insert("en".to_string(), Self::create_bundle("en", EN_FTL));
        bundles.insert("zh-CN".to_string(), Self::create_bundle("zh-CN", ZH_CN_FTL));

        let current_lang = match lang {
            Some(l) if bundles.contains_key(l) => l.to_string(),
            Some(l) => {
                tracing::warn!("unsupported language '{}', falling back to 'en'", l);
                "en".to_string()
            }
            None => "en".to_string(),
        };

        Self {
            current_lang,
            bundles,
        }
    }

    fn create_bundle(lang: &str, source: &str) -> FluentBundle<FluentResource> {
        let langid = match lang {
            "en" => unic_langid::langid!("en"),
            "zh-CN" => unic_langid::langid!("zh-CN"),
            _ => unic_langid::langid!("en"),
        };
        let resource = FluentResource::try_new(source.to_string()).expect("FTL parse error");
        let mut bundle = FluentBundle::new_concurrent(vec![langid]);
        bundle
            .add_resource(resource)
            .expect("Failed to add FTL resource");
        bundle.set_use_isolating(false);
        bundle
    }

    pub fn tr(&self, key: &str) -> String {
        self.format_key(key, None)
    }

    pub fn tr_args(&self, key: &str, args: &[(String, FluentValue<'_>)]) -> String {
        let mut fa = FluentArgs::new();
        for (k, v) in args {
            fa.set(k.as_str(), v.clone());
        }
        self.format_key(key, Some(&fa))
    }

    fn format_key(&self, key: &str, args: Option<&FluentArgs<'_>>) -> String {
        if let Some(value) = self.format_in_bundle(&self.current_lang, key, args) {
            return value;
        }
        if self.current_lang != "en" {
            if let Some(value) = self.format_in_bundle("en", key, args) {
                return value;
            }
        }
        key.to_string()
    }

    fn format_in_bundle(
        &self,
        lang: &str,
        key: &str,
        args: Option<&FluentArgs<'_>>,
    ) -> Option<String> {
        let bundle = self.bundles.get(lang)?;
        let msg = bundle.get_message(key)?;
        let pattern = msg.value()?;
        let mut errors = vec![];
        let value = bundle.format_pattern(pattern, args, &mut errors);
        if !errors.is_empty() {
            tracing::warn!("FTL format errors for key '{}': {:?}", key, errors);
        }
        Some(value.to_string())
    }

    pub fn switch(&mut self, lang: &str) -> anyhow::Result<()> {
        if self.bundles.contains_key(lang) {
            self.current_lang = lang.to_string();
            Ok(())
        } else {
            Err(anyhow::anyhow!("unsupported language: {}", lang))
        }
    }

    pub fn available_langs(&self) -> Vec<&str> {
        self.bundles.keys().map(|s| s.as_str()).collect()
    }

    pub fn current_lang(&self) -> &str {
        self.current_lang.as_str()
    }
}

impl Default for LcRegistry {
    fn default() -> Self {
        Self::new(None)
    }
}

#[cfg(test)]
mod tests {
    include!("mod_test.rs");
}
