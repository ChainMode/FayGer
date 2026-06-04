//! 可信根（TrustedRoot）与可信根存储（TrustStore）。
//!
//! M4 的 TrustStore 是一组裸公钥（`key_ref → public_key`）与一个强制签名开关，
//! 全部为内存对象（不引入 X.509 / 证书链 / 撤销列表，见需求 §9）。
//!
//! 可信根更新可见性天然成立：[`crate::load::load_signed`] 每次调用读取传入的当前
//! TrustStore；调用方在两次加载之间 [`TrustStore::update_roots`]，后一次加载即用新集合。

/// 一条可信根：`key_ref` 对应 BuF 签名块的 `public_key_ref`，`public_key` 是 ed25519 公钥字节。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrustedRoot {
    /// 公钥引用标识，与 `SignatureBlock.public_key_ref` 对应。
    pub key_ref: String,
    /// ed25519 公钥字节（32 字节）。
    pub public_key: Vec<u8>,
}

impl TrustedRoot {
    /// 便利构造。
    #[must_use]
    pub fn new(key_ref: impl Into<String>, public_key: impl Into<Vec<u8>>) -> Self {
        Self {
            key_ref: key_ref.into(),
            public_key: public_key.into(),
        }
    }
}

/// 可信根集合 + 强制签名开关。内存对象。
#[derive(Debug, Clone, Default)]
pub struct TrustStore {
    roots: Vec<TrustedRoot>,
    enforce: bool,
}

impl TrustStore {
    /// 用初始可信根集合与初始强制签名开关构造。
    #[must_use]
    pub const fn new(roots: Vec<TrustedRoot>, enforce: bool) -> Self {
        Self { roots, enforce }
    }

    /// 当前可信根集合。
    #[must_use]
    pub fn roots(&self) -> &[TrustedRoot] {
        &self.roots
    }

    /// 强制签名开关。
    #[must_use]
    pub const fn enforce_signature(&self) -> bool {
        self.enforce
    }

    /// 设置强制签名开关。
    pub const fn set_enforce(&mut self, enforce: bool) {
        self.enforce = enforce;
    }

    /// 更新可信根集合。更新在该调用之后发起的加载中生效。
    pub fn update_roots(&mut self, roots: Vec<TrustedRoot>) {
        self.roots = roots;
    }

    /// 按 `key_ref` 查找可信根。
    #[must_use]
    pub fn find(&self, key_ref: &str) -> Option<&TrustedRoot> {
        self.roots.iter().find(|r| r.key_ref == key_ref)
    }
}

#[cfg(test)]
mod tests {
    use super::{TrustStore, TrustedRoot};

    #[test]
    fn find_hits_and_misses() {
        let ts = TrustStore::new(vec![TrustedRoot::new("k1", vec![1u8; 32])], false);
        assert!(ts.find("k1").is_some());
        assert!(ts.find("k2").is_none());
    }

    #[test]
    fn update_roots_reflects_new_set() {
        let mut ts = TrustStore::new(vec![TrustedRoot::new("k1", vec![1u8; 32])], false);
        assert!(ts.find("k1").is_some());
        ts.update_roots(vec![TrustedRoot::new("k2", vec![2u8; 32])]);
        assert!(ts.find("k1").is_none());
        assert!(ts.find("k2").is_some());
    }

    #[test]
    fn enforce_toggle() {
        let mut ts = TrustStore::new(vec![], false);
        assert!(!ts.enforce_signature());
        ts.set_enforce(true);
        assert!(ts.enforce_signature());
    }

    #[test]
    fn roots_accessor() {
        let ts = TrustStore::new(vec![TrustedRoot::new("k1", vec![0u8; 32])], true);
        assert_eq!(ts.roots().len(), 1);
        assert!(ts.enforce_signature());
    }
}
