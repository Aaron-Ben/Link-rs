use bytes::{BytesMut, BufMut, Buf, Bytes};
use std::fmt;

#[derive(Debug)]
enum FrameError {
    TooShort,                  // 缓冲区长度不足
    InvalidTotalLen(u32, usize), // 总长度不合法（声明的长度，实际缓冲区长度）
    UnknownFrameType(u8),      // 未知的帧类型
    TotalLenOverflow(usize),   // 总长度超过 u32 最大值（4字节上限）
}

impl fmt::Display for FrameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FrameError::TooShort => write!(f, "buffer is too short to parse frame"),
            FrameError::InvalidTotalLen(declared, actual) => write!(
                f, "invalid total length: declared {} but actual buffer length is {}",
                declared, actual
            ),
            FrameError::UnknownFrameType(t) => write!(f, "unknown frame type: {}", t),
            FrameError::TotalLenOverflow(len) => write!(
                f, "total length {} exceeds u32 maximum ({}), cannot encode",
                len, u32::MAX
            ),
        }
    }
}

// 帧类型（L4 控制/数据标识）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FrameType {
    Data = 0,
    Ack = 1,
    Syn = 2,
}

// L4 传输段（原 Frame 命名，本质是 L4 Segment）
#[derive(Debug, Clone)]
struct Segment {
    frame_type: FrameType,
    seq: u64,
    data: Bytes, // 改用 Bytes 避免拷贝，提升性能
}

impl Segment {
    // 安全构造方法：强制数据长度与逻辑一致，避免手动错误
    fn new(frame_type: FrameType, seq: u64, data: Vec<u8>) -> Self {
        Self {
            frame_type,
            seq,
            data: Bytes::from(data), // Vec<u8> 转 Bytes（零拷贝）
        }
    }

    // 头部固定长度：4(total_len) + 1(type) + 8(seq) = 13 字节（移除了冗余的 len 字段）
    const FIXED_HEADER_LEN: usize = 4 + 1 + 8;

    // 编码：Segment -> Result<BytesMut, FrameError>（返回 Result 处理溢出）
    fn encode(&self) -> Result<BytesMut, FrameError> {
        let data_len = self.data.len();
        let total_len = Self::FIXED_HEADER_LEN + data_len;

        // 关键修复1：将 total_len（usize）安全转为 u32（避免溢出和类型不匹配）
        let total_len_u32 = u32::try_from(total_len)
            .map_err(|_| FrameError::TotalLenOverflow(total_len))?;

        // 精准预分配内存（用 usize 类型的 total_len，内存分配需要 usize）
        let mut buf = BytesMut::with_capacity(total_len);

        // 1. 写入总长度占位（4字节）
        buf.put_u32(0);
        // 2. 写入帧类型（u8）
        buf.put_u8(self.frame_type as u8);
        // 3. 写入序列号（u64，大端序）
        buf.put_u64(self.seq);
        // 4. 写入数据体
        buf.put_slice(&self.data);

        // 关键修复2：用 u32 转 4 字节大端序（与目标切片长度一致）
        buf[0..4].copy_from_slice(&total_len_u32.to_be_bytes());

        Ok(buf)
    }

    // 解码：&[u8] -> Result<Segment, FrameError>
    fn decode(buf: &[u8]) -> Result<Self, FrameError> {
        if buf.len() < 4 {
            return Err(FrameError::TooShort);
        }

        let mut slice = &buf[..];
        let total_len_declared = slice.get_u32() as usize; // 读取 4 字节 u32，转 usize 方便计算

        // 校验：总长度不能超过缓冲区实际长度，且至少包含固定头部
        if total_len_declared > buf.len() || total_len_declared < Self::FIXED_HEADER_LEN {
            return Err(FrameError::InvalidTotalLen(
                total_len_declared as u32,
                buf.len()
            ));
        }

        // 读取帧类型
        let frame_type = match slice.get_u8() {
            0 => FrameType::Data,
            1 => FrameType::Ack,
            2 => FrameType::Syn,
            t => return Err(FrameError::UnknownFrameType(t)),
        };

        // 读取序列号
        let seq = slice.get_u64();

        // 读取数据体（长度 = 声明的总长度 - 固定头部长度）
        let data_len = total_len_declared - Self::FIXED_HEADER_LEN;
        let data = Bytes::copy_from_slice(&slice[..data_len]);

        Ok(Self {
            frame_type,
            seq,
            data,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_decode() {
        // 1. 构造段
        let segment = Segment::new(FrameType::Syn, 12345, vec![0x11, 0x22, 0x33]);

        // 2. 编码（处理 Result）
        let encoded = segment.encode().unwrap();

        // 3. 解码
        let decoded = Segment::decode(&encoded).unwrap();

        // 4. 验证
        assert_eq!(decoded.frame_type, FrameType::Syn);
        assert_eq!(decoded.seq, 12345);
        assert_eq!(decoded.data, Bytes::from(vec![0x11, 0x22, 0x33]));
    }

    #[test]
    fn test_decode_invalid_type() {
        // 构造一个帧类型为 3 的非法数据
        let mut buf = BytesMut::new();
        buf.put_u32(13); // 总长度 = 固定头部长度（13），无数据
        buf.put_u8(3);   // 非法类型
        buf.put_u64(0);  // 序列号

        let result = Segment::decode(&buf);
        assert!(matches!(result, Err(FrameError::UnknownFrameType(3))));
    }

    #[test]
    fn test_decode_invalid_total_len() {
        // 总长度声明为 100，但实际缓冲区只有 13 字节
        let mut buf = BytesMut::new();
        buf.put_u32(100); // 非法总长度
        buf.put_u8(0);
        buf.put_u64(0);

        let result = Segment::decode(&buf);
        assert!(matches!(result, Err(FrameError::InvalidTotalLen(100, 13))));
    }

    #[test]
    fn test_encode_total_len_overflow() {
        // 构造超大数据（超过 u32::MAX 长度）
        let big_data = vec![0; (u32::MAX as usize) + 1]; // 数据长度 = 4294967296（u32最大值+1）
        let segment = Segment::new(FrameType::Data, 0, big_data);

        // 编码应返回溢出错误
        let result = segment.encode();
        assert!(matches!(result, Err(FrameError::TotalLenOverflow(_))));
    }
}