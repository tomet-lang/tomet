//! The initial builtin function set: `add`/`sub`/`mul`/`div`/`mod`, all
//! binary and numeric-only. `add`/`sub`/`mul` stay `Value::Int` when both
//! operands are `Int` (promoting to `Value::Float` only when either
//! operand is, or on `i64` overflow); `div` always produces a `Float`
//! (true division, not integer/floor division -- less surprising for a
//! document-display context than silent truncation); `mod` keeps integer
//! remainder for two `Int`s, otherwise floating-point remainder.

use tomet_ast::Value;

use crate::error::ComputeError;

pub(crate) fn call(name: &str, args: &[Value]) -> Result<Value, ComputeError> {
    match name {
        "add" | "sub" | "mul" | "div" | "mod" => {
            let (a, b) = require_two(name, args)?;
            let a = as_number(name, a)?;
            let b = as_number(name, b)?;
            match name {
                "add" => Ok(checked_or_float(a, b, i64::checked_add, |x, y| x + y)),
                "sub" => Ok(checked_or_float(a, b, i64::checked_sub, |x, y| x - y)),
                "mul" => Ok(checked_or_float(a, b, i64::checked_mul, |x, y| x * y)),
                "div" => div(a, b),
                "mod" => rem(a, b),
                _ => unreachable!(),
            }
        }
        "date" => call_date(args),
        "time" => call_time(args),
        "uuid" => call_uuid(args),
        "unicode" => call_unicode(args),
        "emoji" => call_emoji(args),
        "tm" => call_tm(args),
        "ref" => call_ref(args),
        _ => Err(ComputeError::UnknownFunction(name.to_string())),
    }
}

fn call_uuid(args: &[Value]) -> Result<Value, ComputeError> {
    match args {
        [] => Ok(Value::String(uuid::Uuid::new_v4().to_string())),
        [Value::String(opt)] if opt == "simple" => {
            Ok(Value::String(uuid::Uuid::new_v4().simple().to_string()))
        }
        [Value::String(opt)] if opt == "nil" => {
            Ok(Value::String(uuid::Uuid::nil().to_string()))
        }
        _ => Ok(Value::String(uuid::Uuid::new_v4().to_string())),
    }
}

fn call_time(args: &[Value]) -> Result<Value, ComputeError> {
    let now = time::OffsetDateTime::now_utc().time();
    match args {
        [] => Ok(Value::String(format!(
            "{:02}:{:02}:{:02}",
            now.hour(),
            now.minute(),
            now.second()
        ))),
        [Value::String(fmt)] => {
            let formatted = fmt
                .replace("HH", &format!("{:02}", now.hour()))
                .replace("MM", &format!("{:02}", now.minute()))
                .replace("SS", &format!("{:02}", now.second()));
            Ok(Value::String(formatted))
        }
        _ => Ok(Value::String(format!(
            "{:02}:{:02}:{:02}",
            now.hour(),
            now.minute(),
            now.second()
        ))),
    }
}

fn call_date(args: &[Value]) -> Result<Value, ComputeError> {
    let today = time::OffsetDateTime::now_utc().date();
    let (y, m, d) = (
        format!("{:04}", today.year()),
        format!("{:02}", today.month() as u8),
        format!("{:02}", today.day()),
    );
    match args {
        [] => Ok(Value::String(format!("{y}-{m}-{d}"))),
        [Value::String(fmt)]
            if fmt.contains("YYYY")
                || fmt.contains("MM")
                || fmt.contains("DD")
                || fmt.contains("YY")
                || fmt.contains("年")
                || fmt.contains("月")
                || fmt.contains("日") =>
        {
            let formatted = fmt
                .replace("YYYY", &y)
                .replace("YY", if y.len() >= 2 { &y[y.len() - 2..] } else { &y })
                .replace("MM", &m)
                .replace("DD", &d);
            Ok(Value::String(formatted))
        }
        [Value::String(s)] => Ok(Value::String(s.clone())),
        [Value::String(s), Value::String(fmt)] => {
            let s_trimmed = s.trim();
            let parts: Vec<&str> = s_trimmed.split(&['-', '/', '.'][..]).collect();
            if parts.len() == 3 {
                let (py, pm, pd) = (parts[0], parts[1], parts[2]);
                let formatted = fmt
                    .replace("YYYY", py)
                    .replace("YY", if py.len() >= 2 { &py[py.len() - 2..] } else { py })
                    .replace("MM", pm)
                    .replace("DD", pd);
                Ok(Value::String(formatted))
            } else {
                Ok(Value::String(s.clone()))
            }
        }
        other => Err(ComputeError::WrongArgCount {
            function: "date".to_string(),
            expected: 1,
            got: other.len(),
        }),
    }
}

fn call_unicode(args: &[Value]) -> Result<Value, ComputeError> {
    let val = match args {
        [v] => v,
        other => {
            return Err(ComputeError::WrongArgCount {
                function: "unicode".to_string(),
                expected: 1,
                got: other.len(),
            });
        }
    };
    let code = match val {
        Value::Int(i) => *i as u32,
        Value::String(s) => {
            let clean = s
                .trim()
                .trim_start_matches("U+")
                .trim_start_matches("u+")
                .trim_start_matches("0x")
                .trim_start_matches("0X");
            u32::from_str_radix(clean, 16).map_err(|_| ComputeError::NotNumeric {
                function: "unicode".to_string(),
                value: val.clone(),
            })?
        }
        _ => {
            return Err(ComputeError::NotNumeric {
                function: "unicode".to_string(),
                value: val.clone(),
            });
        }
    };
    let c = char::from_u32(code).ok_or_else(|| ComputeError::NotNumeric {
        function: "unicode".to_string(),
        value: val.clone(),
    })?;
    Ok(Value::String(c.to_string()))
}

fn call_emoji(args: &[Value]) -> Result<Value, ComputeError> {
    let val = match args {
        [v] => v,
        other => {
            return Err(ComputeError::WrongArgCount {
                function: "emoji".to_string(),
                expected: 1,
                got: other.len(),
            });
        }
    };
    let name = match val {
        Value::String(s) => s.trim().trim_matches(':'),
        _ => {
            return Err(ComputeError::NotNumeric {
                function: "emoji".to_string(),
                value: val.clone(),
            });
        }
    };
    let emoji = match name {
        "sparkles" | "sparkle" => "✨",
        "tada" | "party" => "🎉",
        "check" | "white_check_mark" => "✅",
        "warning" | "warn" => "⚠️",
        "fire" | "flame" => "🔥",
        "rocket" => "🚀",
        "smile" | "grinning" => "😊",
        "heart" => "❤️",
        "star" => "⭐",
        "bulb" | "idea" => "💡",
        "memo" | "pencil" => "📝",
        "gear" | "cog" => "⚙️",
        "link" => "🔗",
        "book" => "📖",
        "pin" | "pushpin" => "📌",
        "wave" => "👋",
        "bug" => "🐛",
        "cross" | "x" => "❌",
        "info" | "information_source" => "ℹ️",
        "eyes" => "👀",
        "clap" => "👏",
        "thumbsup" | "+1" => "👍",
        "thumbsdown" | "-1" => "👎",
        "zap" | "lightning" => "⚡",
        "art" => "🎨",
        "construction" | "wip" => "🚧",
        "lock" => "🔒",
        "unlock" => "🔓",
        "key" => "🔑",
        "mag" | "search" => "🔍",
        "sun" => "☀️",
        "moon" => "🌙",
        "coffee" => "☕",
        other => other,
    };
    Ok(Value::String(emoji.to_string()))
}

fn call_tm(args: &[Value]) -> Result<Value, ComputeError> {
    match args {
        [Value::String(path)] => Ok(Value::String(format!("tm:{path}"))),
        [Value::String(path), Value::String(sec)] => Ok(Value::String(format!("tm:{path}#{sec}"))),
        other => Err(ComputeError::WrongArgCount {
            function: "tm".to_string(),
            expected: 1,
            got: other.len(),
        }),
    }
}

fn call_ref(args: &[Value]) -> Result<Value, ComputeError> {
    match args {
        [Value::String(target)] => Ok(Value::String(format!("ref:{target}"))),
        other => Err(ComputeError::WrongArgCount {
            function: "ref".to_string(),
            expected: 1,
            got: other.len(),
        }),
    }
}

#[derive(Clone, Copy)]
enum Number {
    Int(i64),
    Float(f64),
}

impl Number {
    fn as_f64(self) -> f64 {
        match self {
            Number::Int(i) => i as f64,
            Number::Float(f) => f,
        }
    }
}

fn require_two<'a>(name: &str, args: &'a [Value]) -> Result<(&'a Value, &'a Value), ComputeError> {
    match args {
        [a, b] => Ok((a, b)),
        other => Err(ComputeError::WrongArgCount {
            function: name.to_string(),
            expected: 2,
            got: other.len(),
        }),
    }
}

fn as_number(name: &str, value: &Value) -> Result<Number, ComputeError> {
    match value {
        Value::Int(i) => Ok(Number::Int(*i)),
        Value::Float(f) => Ok(Number::Float(*f)),
        other => Err(ComputeError::NotNumeric {
            function: name.to_string(),
            value: other.clone(),
        }),
    }
}

/// `Int op Int` stays `Int` via `checked`, falling back to `Float` both
/// when either operand already is one and on overflow (rather than
/// panicking/wrapping) -- `int_op`/`float_op` compute the same operation
/// for each case.
fn checked_or_float(
    a: Number,
    b: Number,
    int_op: fn(i64, i64) -> Option<i64>,
    float_op: fn(f64, f64) -> f64,
) -> Value {
    match (a, b) {
        (Number::Int(x), Number::Int(y)) => match int_op(x, y) {
            Some(result) => Value::Int(result),
            None => Value::Float(float_op(x as f64, y as f64)),
        },
        (a, b) => Value::Float(float_op(a.as_f64(), b.as_f64())),
    }
}

fn div(a: Number, b: Number) -> Result<Value, ComputeError> {
    let divisor = b.as_f64();
    if divisor == 0.0 {
        return Err(ComputeError::DivisionByZero);
    }
    Ok(Value::Float(a.as_f64() / divisor))
}

fn rem(a: Number, b: Number) -> Result<Value, ComputeError> {
    match (a, b) {
        (Number::Int(x), Number::Int(y)) => {
            if y == 0 {
                return Err(ComputeError::DivisionByZero);
            }
            Ok(Value::Int(x % y))
        }
        (a, b) => {
            let divisor = b.as_f64();
            if divisor == 0.0 {
                return Err(ComputeError::DivisionByZero);
            }
            Ok(Value::Float(a.as_f64() % divisor))
        }
    }
}
