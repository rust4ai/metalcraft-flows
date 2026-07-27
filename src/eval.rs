//! Deterministic predicate evaluation for [`conditional`](crate::CoreNodeType::Conditional)
//! nodes.
//!
//! Pure and side-effect free: given an operator, the actual value read from flow
//! state, and an expected value, decide whether the predicate holds. `gt`/`lt`
//! compare **numerically** when both operands parse as numbers — unlike a naive
//! string compare where `"18" > "50"` would be true.

use serde_json::Value;

/// A comparison operator on a [`crate::nodes::Condition`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Operator {
    /// Equal (type-aware: numbers compare numerically, else string-equal).
    Equals,
    /// Not equal.
    NotEquals,
    /// `actual` (as string) contains `expected` (as string).
    Contains,
    /// `actual` (as string) starts with `expected` (as string).
    StartsWith,
    /// `actual` (as string) ends with `expected` (as string).
    EndsWith,
    /// Numeric greater-than.
    Gt,
    /// Numeric less-than.
    Lt,
    /// `actual` is present and not null.
    Exists,
    /// `actual` is truthy (non-empty string, non-zero number, `true`, non-empty
    /// array/object).
    Truthy,
    /// `actual` (as string) matches `expected` as a regular expression.
    ///
    /// Requires the `regex` crate feature; without it this always evaluates to
    /// `false`.
    Matches,
}

impl Operator {
    /// Parse an operator's wire-format string.
    pub fn from_wire(s: &str) -> Option<Self> {
        match s {
            "equals" => Some(Operator::Equals),
            "not_equals" => Some(Operator::NotEquals),
            "contains" => Some(Operator::Contains),
            "starts_with" => Some(Operator::StartsWith),
            "ends_with" => Some(Operator::EndsWith),
            "gt" => Some(Operator::Gt),
            "lt" => Some(Operator::Lt),
            "exists" => Some(Operator::Exists),
            "truthy" => Some(Operator::Truthy),
            "matches" => Some(Operator::Matches),
            _ => None,
        }
    }

    /// The operator's wire-format string.
    pub fn as_str(self) -> &'static str {
        match self {
            Operator::Equals => "equals",
            Operator::NotEquals => "not_equals",
            Operator::Contains => "contains",
            Operator::StartsWith => "starts_with",
            Operator::EndsWith => "ends_with",
            Operator::Gt => "gt",
            Operator::Lt => "lt",
            Operator::Exists => "exists",
            Operator::Truthy => "truthy",
            Operator::Matches => "matches",
        }
    }
}

/// Coerce a JSON value to `f64` if it is a number or a numeric string.
fn as_number(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse::<f64>().ok(),
        _ => None,
    }
}

/// Render a JSON value as a plain string for string-oriented operators
/// (strings pass through unquoted; other scalars use their JSON form).
fn as_text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// Whether a JSON value is "truthy".
fn is_truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().map(|f| f != 0.0).unwrap_or(false),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}

/// Evaluate `op` over `actual` (the value read from state; `None` if the
/// variable is absent) and `expected` (the condition's right-hand `value`,
/// `None` if omitted).
///
/// Unary operators (`exists`, `truthy`) ignore `expected`. `gt`/`lt` require
/// both sides to be numeric and return `false` otherwise.
pub fn evaluate(op: Operator, actual: Option<&Value>, expected: Option<&Value>) -> bool {
    let null = Value::Null;
    let a = actual.unwrap_or(&null);

    match op {
        Operator::Exists => actual.is_some() && !a.is_null(),
        Operator::Truthy => is_truthy(a),
        Operator::Equals | Operator::NotEquals => {
            let eq = match (as_number(a), expected.and_then(as_number)) {
                (Some(x), Some(y)) => x == y,
                _ => match expected {
                    Some(e) => as_text(a) == as_text(e),
                    None => a.is_null(),
                },
            };
            if op == Operator::Equals { eq } else { !eq }
        }
        Operator::Gt | Operator::Lt => {
            match (as_number(a), expected.and_then(as_number)) {
                (Some(x), Some(y)) => {
                    if op == Operator::Gt { x > y } else { x < y }
                }
                _ => false,
            }
        }
        Operator::Contains => match expected {
            Some(e) => as_text(a).contains(&as_text(e)),
            None => false,
        },
        Operator::StartsWith => match expected {
            Some(e) => as_text(a).starts_with(&as_text(e)),
            None => false,
        },
        Operator::EndsWith => match expected {
            Some(e) => as_text(a).ends_with(&as_text(e)),
            None => false,
        },
        Operator::Matches => match expected {
            Some(e) => matches_regex(&as_text(a), &as_text(e)),
            None => false,
        },
    }
}

#[cfg(feature = "regex")]
fn matches_regex(text: &str, pattern: &str) -> bool {
    regex::Regex::new(pattern).map(|re| re.is_match(text)).unwrap_or(false)
}

#[cfg(not(feature = "regex"))]
fn matches_regex(_text: &str, _pattern: &str) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn numeric_gt_lt_not_lexicographic() {
        // The vix bug: "18" > "50" is true as strings. Must be false numerically.
        assert!(!evaluate(Operator::Gt, Some(&json!(18)), Some(&json!(50))));
        assert!(evaluate(Operator::Lt, Some(&json!(18)), Some(&json!(50))));
        assert!(evaluate(Operator::Gt, Some(&json!(75)), Some(&json!(50))));
        // numeric strings coerce too
        assert!(evaluate(Operator::Gt, Some(&json!("75")), Some(&json!("50"))));
        assert!(!evaluate(Operator::Gt, Some(&json!("18")), Some(&json!("50"))));
    }

    #[test]
    fn gt_on_non_numbers_is_false() {
        assert!(!evaluate(Operator::Gt, Some(&json!("hot")), Some(&json!("cold"))));
        assert!(!evaluate(Operator::Gt, None, Some(&json!(1))));
    }

    #[test]
    fn equals_type_aware() {
        assert!(evaluate(Operator::Equals, Some(&json!(5)), Some(&json!("5"))));
        assert!(evaluate(Operator::Equals, Some(&json!("P0")), Some(&json!("P0"))));
        assert!(evaluate(Operator::NotEquals, Some(&json!("P0")), Some(&json!("P1"))));
    }

    #[test]
    fn string_ops() {
        assert!(evaluate(Operator::Contains, Some(&json!("hello world")), Some(&json!("wor"))));
        assert!(evaluate(Operator::StartsWith, Some(&json!("hello")), Some(&json!("he"))));
        assert!(evaluate(Operator::EndsWith, Some(&json!("hello")), Some(&json!("lo"))));
    }

    #[test]
    fn exists_and_truthy() {
        assert!(evaluate(Operator::Exists, Some(&json!("x")), None));
        assert!(!evaluate(Operator::Exists, Some(&json!(null)), None));
        assert!(!evaluate(Operator::Exists, None, None));
        assert!(evaluate(Operator::Truthy, Some(&json!(1)), None));
        assert!(!evaluate(Operator::Truthy, Some(&json!(0)), None));
        assert!(!evaluate(Operator::Truthy, Some(&json!("")), None));
    }

    #[cfg(feature = "regex")]
    #[test]
    fn regex_matches() {
        assert!(evaluate(Operator::Matches, Some(&json!("abc123")), Some(&json!(r"\d+"))));
        assert!(!evaluate(Operator::Matches, Some(&json!("abc")), Some(&json!(r"\d+"))));
    }

    #[test]
    fn operator_round_trips() {
        for op in [
            Operator::Equals, Operator::NotEquals, Operator::Contains,
            Operator::StartsWith, Operator::EndsWith, Operator::Gt, Operator::Lt,
            Operator::Exists, Operator::Truthy, Operator::Matches,
        ] {
            assert_eq!(Operator::from_wire(op.as_str()), Some(op));
        }
    }
}
