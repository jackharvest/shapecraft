//! Typed access to a command's JSON parameters. Every number is checked for being finite.

use glam::{DVec2, DVec3};
use serde::de::DeserializeOwned;
use serde_json::Value;
use shapecraft_doc::ItemId;

use crate::{EngineError, Result};

/// A command's parameters, and the document's variables for numbers given as expressions ("width / 2"). Remembers
/// which keys the command looked at, so callers can tell parameters it doesn't know.
pub struct Params<'a>(&'a Value, &'a [shapecraft_doc::Variable], std::cell::RefCell<Vec<String>>);

fn bad(msg: impl Into<String>) -> EngineError {
    EngineError::BadParams(msg.into())
}

impl<'a> Params<'a> {
    pub fn new(v: &'a Value) -> Self {
        Self(v, &[], Default::default())
    }

    pub fn with_variables(v: &'a Value, vars: &'a [shapecraft_doc::Variable]) -> Self {
        Self(v, vars, Default::default())
    }

    fn get(&self, key: &str) -> Option<&'a Value> {
        let mut seen = self.2.borrow_mut();
        if !seen.iter().any(|k| k == key) {
            seen.push(key.to_string());
        }
        self.0.get(key)
    }

    /// Top-level keys the command never looked at.
    pub fn unread(&self) -> Vec<String> {
        let seen = self.2.borrow();
        self.0.as_object().map(|m| m.keys().filter(|k| !seen.contains(k)).cloned().collect()).unwrap_or_default()
    }

    /// All the parameters as given (marks none as read).
    pub fn all(&self) -> &'a Value {
        self.0
    }

    /// The value as given, null included.
    pub fn raw(&self, key: &str) -> Option<&'a Value> {
        self.get(key)
    }

    pub fn has(&self, key: &str) -> bool {
        self.get(key).is_some_and(|v| !v.is_null())
    }

    pub fn f64(&self, key: &str) -> Result<f64> {
        let v = match self.get(key) {
            Some(Value::String(expr)) => shapecraft_doc::expr::eval(expr, self.1).map_err(|e| bad(format!("'{key}': {e}")))?,
            other => other.and_then(Value::as_f64).ok_or_else(|| bad(format!("'{key}' must be a number")))?,
        };
        if v.is_finite() { Ok(v) } else { Err(bad(format!("'{key}' must be finite"))) }
    }

    pub fn f64_or(&self, key: &str, default: f64) -> Result<f64> {
        if self.has(key) { self.f64(key) } else { Ok(default) }
    }

    pub fn str(&self, key: &str) -> Result<&'a str> {
        self.get(key).and_then(Value::as_str).ok_or_else(|| bad(format!("'{key}' must be a string")))
    }

    pub fn opt_str(&self, key: &str) -> Option<&'a str> {
        self.get(key).and_then(Value::as_str)
    }

    pub fn bool_or(&self, key: &str, default: bool) -> bool {
        self.get(key).and_then(Value::as_bool).unwrap_or(default)
    }

    pub fn id(&self, key: &str) -> Result<ItemId> {
        self.get(key).and_then(Value::as_u64).map(ItemId).ok_or_else(|| bad(format!("'{key}' must be an item id")))
    }

    pub fn opt_id(&self, key: &str) -> Option<ItemId> {
        self.get(key).and_then(Value::as_u64).map(ItemId)
    }

    pub fn ids(&self, key: &str) -> Result<Vec<ItemId>> {
        let arr = self.get(key).and_then(Value::as_array).ok_or_else(|| bad(format!("'{key}' must be a list")))?;
        if arr.len() > 10_000 {
            return Err(bad(format!("'{key}' is too long")));
        }
        arr.iter().map(|v| v.as_u64().map(ItemId).ok_or_else(|| bad(format!("'{key}' must hold item ids")))).collect()
    }

    pub fn u32s(&self, key: &str) -> Result<Vec<u32>> {
        let arr = self.get(key).and_then(Value::as_array).ok_or_else(|| bad(format!("'{key}' must be a list")))?;
        if arr.len() > 10_000 {
            return Err(bad(format!("'{key}' is too long")));
        }
        arr.iter()
            .map(|v| v.as_u64().and_then(|n| u32::try_from(n).ok()).ok_or_else(|| bad(format!("'{key}' must hold indices"))))
            .collect()
    }

    pub fn vec2(&self, key: &str) -> Result<DVec2> {
        let a = self.floats(key, 2)?;
        Ok(DVec2::new(a[0], a[1]))
    }

    pub fn vec3(&self, key: &str) -> Result<DVec3> {
        let a = self.floats(key, 3)?;
        Ok(DVec3::new(a[0], a[1], a[2]))
    }

    fn floats(&self, key: &str, n: usize) -> Result<Vec<f64>> {
        let arr = self.get(key).and_then(Value::as_array).ok_or_else(|| bad(format!("'{key}' must be [{n} numbers]")))?;
        if arr.len() != n {
            return Err(bad(format!("'{key}' must be [{n} numbers]")));
        }
        arr.iter().map(|v| v.as_f64().filter(|f| f.is_finite()).ok_or_else(|| bad(format!("'{key}' must hold finite numbers")))).collect()
    }

    pub fn parse<T: DeserializeOwned>(&self, key: &str) -> Result<T> {
        let v = self.get(key).ok_or_else(|| bad(format!("'{key}' is missing")))?;
        serde_json::from_value(v.clone()).map_err(|e| bad(format!("'{key}': {e}")))
    }
}
