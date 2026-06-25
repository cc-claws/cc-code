#[cfg(test)]
mod tests {
    use crate::sync::{
        packer,
        protocol::{SyncItems, SyncPackage},
    };

    #[test]
    fn test_to_msgpack_roundtrip() {
        let pkg = SyncPackage {
            version: 1,
            timestamp: 1715000000,
            items: SyncItems::default(),
        };
        let bytes = pkg.to_msgpack().expect("序列化应成功");
        let unpacked: SyncPackage = rmp_serde::from_slice(&bytes).expect("反序列化应成功");
        assert_eq!(unpacked.version, pkg.version);
        assert_eq!(unpacked.timestamp, pkg.timestamp);
    }

    #[test]
    fn test_pack_produces_chunks() {
        let pkg = SyncPackage {
            version: 1,
            timestamp: 0,
            items: SyncItems::default(),
        };
        let result = packer::pack(&pkg, "test123");
        assert!(result.is_ok());
        let packed = result.unwrap();
        assert!(!packed.chunks.is_empty(), "至少应有 1 个分片");
        // encrypted_size 应等于所有分片大小之和
        let total_in_chunks: usize = packed.chunks.iter().map(|c| c.data.len()).sum();
        assert_eq!(total_in_chunks, packed.encrypted_size);
    }

    #[test]
    fn test_pack_same_code_same_key() {
        let pkg = SyncPackage {
            version: 1,
            timestamp: 0,
            items: SyncItems::default(),
        };
        let result1 = packer::pack(&pkg, "samecode").expect("第一次打包");
        let result2 = packer::pack(&pkg, "samecode").expect("第二次打包");

        // 相同 pair_code 应产生不同密文（因随机 IV）
        assert_ne!(
            result1.chunks[0].data, result2.chunks[0].data,
            "相同明文但不同 IV 应产生不同密文"
        );
        // 但分片数量相同
        assert_eq!(result1.chunks.len(), result2.chunks.len());
    }

    #[test]
    fn test_compute_checksum_deterministic() {
        let data = b"hello world";
        let cs1 = packer::compute_checksum(data);
        let cs2 = packer::compute_checksum(data);
        assert_eq!(cs1, cs2);
        assert_eq!(cs1.len(), 64, "SHA-256 十六进制为 64 字符");
    }

    #[test]
    fn test_compute_checksum_different_data() {
        let cs1 = packer::compute_checksum(b"hello");
        let cs2 = packer::compute_checksum(b"world");
        assert_ne!(cs1, cs2);
    }
}
