//! `MessagePack`, as much as Nvim's RPC uses: a [`Value`] written to bytes
//! ([`encode`]) and read from them ([`decode`]). Pure. Hand-written: the
//! format is small and the app has no other use for a crate.

/// One `MessagePack` value. An integer past an `i64` is its largest.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Nil,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
    Bin(Vec<u8>),
    Array(Vec<Value>),
    Map(Vec<(Value, Value)>),
    Ext(i8, Vec<u8>),
}

impl Value {
    pub fn str(text: &str) -> Value {
        Value::Str(text.to_string())
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_int(&self) -> Option<i64> {
        match self {
            Value::Int(n) => Some(*n),
            _ => None,
        }
    }

    pub fn as_usize(&self) -> Option<usize> {
        self.as_int().and_then(|n| usize::try_from(n).ok())
    }

    pub fn as_array(&self) -> Option<&[Value]> {
        match self {
            Value::Array(a) => Some(a),
            _ => None,
        }
    }

    /// The value under the string `key` of a map.
    pub fn get(&self, key: &str) -> Option<&Value> {
        let Value::Map(pairs) = self else { return None };
        pairs.iter().find(|(k, _)| k.as_str() == Some(key)).map(|(_, v)| v)
    }

    /// Whether it is true, or a number other than 0.
    pub fn truthy(&self) -> bool {
        matches!(self, Value::Bool(true)) || self.as_int().is_some_and(|n| n != 0)
    }
}

/// Appends `value` to `out`.
#[expect(clippy::cast_possible_truncation, reason = "each cast follows a range check")]
#[expect(clippy::cast_sign_loss, reason = "each cast follows a range check")]
pub fn encode(value: &Value, out: &mut Vec<u8>) {
    let len = |out: &mut Vec<u8>, n: usize, short: Option<(u8, usize)>, wide: [u8; 2]| match short {
        Some((tag, most)) if n <= most => out.push(tag | n as u8),
        _ if n <= 0xffff => {
            out.push(wide[0]);
            out.extend((n as u16).to_be_bytes());
        }
        _ => {
            out.push(wide[1]);
            out.extend((n as u32).to_be_bytes());
        }
    };
    match value {
        Value::Nil => out.push(0xc0),
        Value::Bool(b) => out.push(if *b { 0xc3 } else { 0xc2 }),
        Value::Int(n) => match *n {
            0..=127 => out.push(*n as u8),
            -32..=-1 => out.push(*n as i8 as u8),
            n if n >= 0 => {
                out.push(0xcf);
                out.extend((n as u64).to_be_bytes());
            }
            n => {
                out.push(0xd3);
                out.extend(n.to_be_bytes());
            }
        },
        Value::Float(f) => {
            out.push(0xcb);
            out.extend(f.to_be_bytes());
        }
        Value::Str(s) => {
            if (32..=255).contains(&s.len()) {
                out.extend([0xd9, s.len() as u8]);
            } else {
                len(out, s.len(), Some((0xa0, 31)), [0xda, 0xdb]);
            }
            out.extend(s.as_bytes());
        }
        Value::Bin(b) => {
            len(out, b.len(), None, [0xc5, 0xc6]);
            out.extend(b);
        }
        Value::Array(items) => {
            len(out, items.len(), Some((0x90, 15)), [0xdc, 0xdd]);
            for item in items {
                encode(item, out);
            }
        }
        Value::Map(pairs) => {
            len(out, pairs.len(), Some((0x80, 15)), [0xde, 0xdf]);
            for (k, v) in pairs {
                encode(k, out);
                encode(v, out);
            }
        }
        Value::Ext(kind, data) => {
            out.push(0xc9);
            out.extend((data.len() as u32).to_be_bytes());
            out.push(*kind as u8);
            out.extend(data);
        }
    }
}

/// Why bytes are not a value yet.
#[derive(Debug, PartialEq)]
pub enum Short {
    /// More bytes are to come.
    More,
    /// A byte no value starts with.
    Bad(u8),
}

/// The first value of `bytes` and how many bytes it took.
pub fn decode(bytes: &[u8]) -> Result<(Value, usize), Short> {
    let mut at = 0;
    let value = read(bytes, &mut at, 0)?;
    Ok((value, at))
}

fn take<'a>(bytes: &'a [u8], at: &mut usize, n: usize) -> Result<&'a [u8], Short> {
    let end = at.checked_add(n).ok_or(Short::More)?;
    let part = bytes.get(*at..end).ok_or(Short::More)?;
    *at = end;
    Ok(part)
}

fn uint(bytes: &[u8], at: &mut usize, n: usize) -> Result<u64, Short> {
    Ok(take(bytes, at, n)?.iter().fold(0, |sum, b| (sum << 8) | u64::from(*b)))
}

fn size(bytes: &[u8], at: &mut usize, n: usize) -> Result<usize, Short> {
    uint(bytes, at, n).map(|n| usize::try_from(n).unwrap_or(usize::MAX))
}

fn text(bytes: &[u8], at: &mut usize, n: usize) -> Result<Value, Short> {
    Ok(Value::Str(String::from_utf8_lossy(take(bytes, at, n)?).into_owned()))
}

fn list(bytes: &[u8], at: &mut usize, n: usize, deep: u8) -> Result<Value, Short> {
    let mut items = Vec::with_capacity(n.min(4096));
    for _ in 0..n {
        items.push(read(bytes, at, deep + 1)?);
    }
    Ok(Value::Array(items))
}

fn map(bytes: &[u8], at: &mut usize, n: usize, deep: u8) -> Result<Value, Short> {
    let mut pairs = Vec::with_capacity(n.min(4096));
    for _ in 0..n {
        pairs.push((read(bytes, at, deep + 1)?, read(bytes, at, deep + 1)?));
    }
    Ok(Value::Map(pairs))
}

fn ext(bytes: &[u8], at: &mut usize, n: usize) -> Result<Value, Short> {
    let kind = take(bytes, at, 1)?[0].cast_signed();
    Ok(Value::Ext(kind, take(bytes, at, n)?.to_vec()))
}

#[expect(clippy::cast_possible_wrap, reason = "MessagePack's signed integers are two's complement")]
#[expect(clippy::cast_possible_truncation, reason = "the width read is the width cast to")]
fn read(bytes: &[u8], at: &mut usize, deep: u8) -> Result<Value, Short> {
    let tag = take(bytes, at, 1)?[0];
    if deep > 64 {
        return Err(Short::Bad(tag));
    }
    Ok(match tag {
        0x00..=0x7f => Value::Int(i64::from(tag)),
        0x80..=0x8f => map(bytes, at, usize::from(tag & 0x0f), deep)?,
        0x90..=0x9f => list(bytes, at, usize::from(tag & 0x0f), deep)?,
        0xa0..=0xbf => text(bytes, at, usize::from(tag & 0x1f))?,
        0xc0 => Value::Nil,
        0xc2 => Value::Bool(false),
        0xc3 => Value::Bool(true),
        0xc4..=0xc6 => {
            let n = size(bytes, at, 1 << (tag - 0xc4))?;
            Value::Bin(take(bytes, at, n)?.to_vec())
        }
        0xc7..=0xc9 => {
            let n = size(bytes, at, 1 << (tag - 0xc7))?;
            ext(bytes, at, n)?
        }
        0xca => Value::Float(f64::from(f32::from_bits(uint(bytes, at, 4)? as u32))),
        0xcb => Value::Float(f64::from_bits(uint(bytes, at, 8)?)),
        0xcc..=0xcf => {
            let n = uint(bytes, at, 1 << (tag - 0xcc))?;
            Value::Int(i64::try_from(n).unwrap_or(i64::MAX))
        }
        0xd0 => Value::Int(i64::from(uint(bytes, at, 1)? as u8 as i8)),
        0xd1 => Value::Int(i64::from(uint(bytes, at, 2)? as u16 as i16)),
        0xd2 => Value::Int(i64::from(uint(bytes, at, 4)? as u32 as i32)),
        0xd3 => Value::Int(uint(bytes, at, 8)? as i64),
        0xd4..=0xd8 => ext(bytes, at, 1 << (tag - 0xd4))?,
        0xd9..=0xdb => {
            let n = size(bytes, at, 1 << (tag - 0xd9))?;
            text(bytes, at, n)?
        }
        0xdc | 0xdd => {
            let n = size(bytes, at, 2 << (tag - 0xdc))?;
            list(bytes, at, n, deep)?
        }
        0xde | 0xdf => {
            let n = size(bytes, at, 2 << (tag - 0xde))?;
            map(bytes, at, n, deep)?
        }
        0xe0..=0xff => Value::Int(i64::from(tag as i8)),
        0xc1 => return Err(Short::Bad(tag)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_value_written_reads_back_and_a_cut_one_asks_for_more() {
        let long = "x".repeat(300);
        let value = Value::Array(vec![
            Value::Int(0),
            Value::Int(-1),
            Value::Int(70000),
            Value::Int(-70000),
            Value::str("grid_line"),
            Value::str(&long),
            Value::Bool(true),
            Value::Nil,
            Value::Float(1.5),
            Value::Map(vec![(Value::str("ext_linegrid"), Value::Bool(true))]),
            Value::Array((0..20).map(Value::Int).collect()),
        ]);
        let mut bytes = Vec::new();
        encode(&value, &mut bytes);
        assert_eq!(decode(&bytes), Ok((value, bytes.len())));
        assert_eq!(decode(&bytes[..bytes.len() - 1]), Err(Short::More));
        assert_eq!(decode(&[0xc1]), Err(Short::Bad(0xc1)));
    }
}
