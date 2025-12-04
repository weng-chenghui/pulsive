//! Expression engine for evaluating conditions
//!
//! Expressions are loaded from RON scripts and evaluated at runtime
//! against the current game state.

use crate::{DefId, Entity, EntityRef, EntityStore, Error, GameRng, Result, Value, ValueMap};
use serde::{Deserialize, Serialize};

/// An expression that can be evaluated to produce a Value
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Expr {
    // === Literals ===
    /// A literal value
    Literal(Value),

    // === Property Access ===
    /// Read a property from the target entity
    Property(String),
    /// Read a property from a specific entity
    EntityProperty(EntityRef, String),
    /// Read a global property
    Global(String),
    /// Read a parameter passed to the current context
    Param(String),

    // === Arithmetic ===
    /// Add two expressions
    Add(Box<Expr>, Box<Expr>),
    /// Subtract second from first
    Sub(Box<Expr>, Box<Expr>),
    /// Multiply two expressions
    Mul(Box<Expr>, Box<Expr>),
    /// Divide first by second
    Div(Box<Expr>, Box<Expr>),
    /// Modulo
    Mod(Box<Expr>, Box<Expr>),
    /// Negate a numeric value
    Neg(Box<Expr>),
    /// Absolute value
    Abs(Box<Expr>),
    /// Minimum of two values
    Min(Box<Expr>, Box<Expr>),
    /// Maximum of two values
    Max(Box<Expr>, Box<Expr>),
    /// Clamp value between min and max
    Clamp(Box<Expr>, Box<Expr>, Box<Expr>),
    /// Floor (round down)
    Floor(Box<Expr>),
    /// Ceiling (round up)
    Ceil(Box<Expr>),
    /// Round to nearest integer
    Round(Box<Expr>),

    // === Comparison ===
    /// Equal
    Eq(Box<Expr>, Box<Expr>),
    /// Not equal
    Ne(Box<Expr>, Box<Expr>),
    /// Less than
    Lt(Box<Expr>, Box<Expr>),
    /// Less than or equal
    Le(Box<Expr>, Box<Expr>),
    /// Greater than
    Gt(Box<Expr>, Box<Expr>),
    /// Greater than or equal
    Ge(Box<Expr>, Box<Expr>),

    // === Logical ===
    /// Logical AND (all must be true)
    And(Vec<Expr>),
    /// Logical OR (at least one must be true)
    Or(Vec<Expr>),
    /// Logical NOT
    Not(Box<Expr>),

    // === Conditionals ===
    /// If-then-else
    If(Box<Expr>, Box<Expr>, Box<Expr>),

    // === Entity Queries ===
    /// Check if entity has a flag
    HasFlag(DefId),
    /// Check if entity exists
    EntityExists(EntityRef),
    /// Count entities of a kind
    CountEntities(DefId),

    // === Random ===
    /// Random float between 0 and 1
    Random,
    /// Random float in range [min, max)
    RandomRange(Box<Expr>, Box<Expr>),
    /// Random integer in range [min, max]
    RandomInt(Box<Expr>, Box<Expr>),
    /// Weighted random choice (returns index)
    WeightedRandom(Vec<Expr>),

    // === String ===
    /// Concatenate strings
    Concat(Vec<Expr>),
    /// Format a string with values
    Format(String, Vec<Expr>),
}

/// Context for evaluating expressions
pub struct EvalContext<'a> {
    /// The target entity (if any)
    pub target: Option<&'a Entity>,
    /// All entities
    pub entities: &'a EntityStore,
    /// Global properties
    pub globals: &'a ValueMap,
    /// Parameters passed to this context
    pub params: &'a ValueMap,
    /// Random number generator
    pub rng: &'a mut GameRng,
}

impl<'a> EvalContext<'a> {
    /// Create a new evaluation context
    pub fn new(
        entities: &'a EntityStore,
        globals: &'a ValueMap,
        params: &'a ValueMap,
        rng: &'a mut GameRng,
    ) -> Self {
        Self {
            target: None,
            entities,
            globals,
            params,
            rng,
        }
    }

    /// Set the target entity
    pub fn with_target(mut self, target: &'a Entity) -> Self {
        self.target = Some(target);
        self
    }
}

impl Expr {
    /// Evaluate this expression in the given context
    pub fn eval(&self, ctx: &mut EvalContext) -> Result<Value> {
        match self {
            // Literals
            Expr::Literal(v) => Ok(v.clone()),

            // Property access
            Expr::Property(name) => {
                let entity = ctx.target.ok_or_else(|| {
                    Error::EvaluationError("No target entity for Property access".to_string())
                })?;
                Ok(entity.get(name).cloned().unwrap_or(Value::Null))
            }
            Expr::EntityProperty(entity_ref, name) => {
                let entity = ctx.entities.resolve(entity_ref);
                Ok(entity
                    .and_then(|e| e.get(name).cloned())
                    .unwrap_or(Value::Null))
            }
            Expr::Global(name) => Ok(ctx.globals.get(name).cloned().unwrap_or(Value::Null)),
            Expr::Param(name) => Ok(ctx.params.get(name).cloned().unwrap_or(Value::Null)),

            // Arithmetic
            Expr::Add(a, b) => {
                let va = a.eval(ctx)?;
                let vb = b.eval(ctx)?;
                numeric_op(&va, &vb, |x, y| x + y)
            }
            Expr::Sub(a, b) => {
                let va = a.eval(ctx)?;
                let vb = b.eval(ctx)?;
                numeric_op(&va, &vb, |x, y| x - y)
            }
            Expr::Mul(a, b) => {
                let va = a.eval(ctx)?;
                let vb = b.eval(ctx)?;
                numeric_op(&va, &vb, |x, y| x * y)
            }
            Expr::Div(a, b) => {
                let va = a.eval(ctx)?;
                let vb = b.eval(ctx)?;
                let fb = vb.as_float().ok_or_else(|| Error::TypeError {
                    expected: "number".to_string(),
                    got: vb.type_name().to_string(),
                })?;
                if fb == 0.0 {
                    return Err(Error::DivisionByZero);
                }
                numeric_op(&va, &vb, |x, y| x / y)
            }
            Expr::Mod(a, b) => {
                let va = a.eval(ctx)?;
                let vb = b.eval(ctx)?;
                numeric_op(&va, &vb, |x, y| x % y)
            }
            Expr::Neg(a) => {
                let va = a.eval(ctx)?;
                let f = va.as_float().ok_or_else(|| Error::TypeError {
                    expected: "number".to_string(),
                    got: va.type_name().to_string(),
                })?;
                Ok(Value::Float(-f))
            }
            Expr::Abs(a) => {
                let va = a.eval(ctx)?;
                let f = va.as_float().ok_or_else(|| Error::TypeError {
                    expected: "number".to_string(),
                    got: va.type_name().to_string(),
                })?;
                Ok(Value::Float(f.abs()))
            }
            Expr::Min(a, b) => {
                let va = a.eval(ctx)?;
                let vb = b.eval(ctx)?;
                let fa = va.as_float().ok_or_else(|| Error::TypeError {
                    expected: "number".to_string(),
                    got: va.type_name().to_string(),
                })?;
                let fb = vb.as_float().ok_or_else(|| Error::TypeError {
                    expected: "number".to_string(),
                    got: vb.type_name().to_string(),
                })?;
                Ok(Value::Float(fa.min(fb)))
            }
            Expr::Max(a, b) => {
                let va = a.eval(ctx)?;
                let vb = b.eval(ctx)?;
                let fa = va.as_float().ok_or_else(|| Error::TypeError {
                    expected: "number".to_string(),
                    got: va.type_name().to_string(),
                })?;
                let fb = vb.as_float().ok_or_else(|| Error::TypeError {
                    expected: "number".to_string(),
                    got: vb.type_name().to_string(),
                })?;
                Ok(Value::Float(fa.max(fb)))
            }
            Expr::Clamp(val, min, max) => {
                let v = val.eval(ctx)?;
                let vmin = min.eval(ctx)?;
                let vmax = max.eval(ctx)?;
                let fv = v.as_float().ok_or_else(|| Error::TypeError {
                    expected: "number".to_string(),
                    got: v.type_name().to_string(),
                })?;
                let fmin = vmin.as_float().ok_or_else(|| Error::TypeError {
                    expected: "number".to_string(),
                    got: vmin.type_name().to_string(),
                })?;
                let fmax = vmax.as_float().ok_or_else(|| Error::TypeError {
                    expected: "number".to_string(),
                    got: vmax.type_name().to_string(),
                })?;
                Ok(Value::Float(fv.clamp(fmin, fmax)))
            }
            Expr::Floor(a) => {
                let va = a.eval(ctx)?;
                let f = va.as_float().ok_or_else(|| Error::TypeError {
                    expected: "number".to_string(),
                    got: va.type_name().to_string(),
                })?;
                Ok(Value::Int(f.floor() as i64))
            }
            Expr::Ceil(a) => {
                let va = a.eval(ctx)?;
                let f = va.as_float().ok_or_else(|| Error::TypeError {
                    expected: "number".to_string(),
                    got: va.type_name().to_string(),
                })?;
                Ok(Value::Int(f.ceil() as i64))
            }
            Expr::Round(a) => {
                let va = a.eval(ctx)?;
                let f = va.as_float().ok_or_else(|| Error::TypeError {
                    expected: "number".to_string(),
                    got: va.type_name().to_string(),
                })?;
                Ok(Value::Int(f.round() as i64))
            }

            // Comparison
            Expr::Eq(a, b) => {
                let va = a.eval(ctx)?;
                let vb = b.eval(ctx)?;
                Ok(Value::Bool(values_equal(&va, &vb)))
            }
            Expr::Ne(a, b) => {
                let va = a.eval(ctx)?;
                let vb = b.eval(ctx)?;
                Ok(Value::Bool(!values_equal(&va, &vb)))
            }
            Expr::Lt(a, b) => {
                let va = a.eval(ctx)?;
                let vb = b.eval(ctx)?;
                compare_values(&va, &vb, |x, y| x < y)
            }
            Expr::Le(a, b) => {
                let va = a.eval(ctx)?;
                let vb = b.eval(ctx)?;
                compare_values(&va, &vb, |x, y| x <= y)
            }
            Expr::Gt(a, b) => {
                let va = a.eval(ctx)?;
                let vb = b.eval(ctx)?;
                compare_values(&va, &vb, |x, y| x > y)
            }
            Expr::Ge(a, b) => {
                let va = a.eval(ctx)?;
                let vb = b.eval(ctx)?;
                compare_values(&va, &vb, |x, y| x >= y)
            }

            // Logical
            Expr::And(exprs) => {
                for expr in exprs {
                    let v = expr.eval(ctx)?;
                    if !v.is_truthy() {
                        return Ok(Value::Bool(false));
                    }
                }
                Ok(Value::Bool(true))
            }
            Expr::Or(exprs) => {
                for expr in exprs {
                    let v = expr.eval(ctx)?;
                    if v.is_truthy() {
                        return Ok(Value::Bool(true));
                    }
                }
                Ok(Value::Bool(false))
            }
            Expr::Not(a) => {
                let va = a.eval(ctx)?;
                Ok(Value::Bool(!va.is_truthy()))
            }

            // Conditionals
            Expr::If(cond, then_expr, else_expr) => {
                let vc = cond.eval(ctx)?;
                if vc.is_truthy() {
                    then_expr.eval(ctx)
                } else {
                    else_expr.eval(ctx)
                }
            }

            // Entity queries
            Expr::HasFlag(flag) => {
                let entity = ctx.target.ok_or_else(|| {
                    Error::EvaluationError("No target entity for HasFlag".to_string())
                })?;
                Ok(Value::Bool(entity.has_flag(flag)))
            }
            Expr::EntityExists(entity_ref) => {
                Ok(Value::Bool(ctx.entities.resolve(entity_ref).is_some()))
            }
            Expr::CountEntities(kind) => {
                let count = ctx.entities.by_kind(kind).count();
                Ok(Value::Int(count as i64))
            }

            // Random
            Expr::Random => Ok(Value::Float(ctx.rng.next_f64())),
            Expr::RandomRange(min, max) => {
                let vmin = min.eval(ctx)?;
                let vmax = max.eval(ctx)?;
                let fmin = vmin.as_float().ok_or_else(|| Error::TypeError {
                    expected: "number".to_string(),
                    got: vmin.type_name().to_string(),
                })?;
                let fmax = vmax.as_float().ok_or_else(|| Error::TypeError {
                    expected: "number".to_string(),
                    got: vmax.type_name().to_string(),
                })?;
                Ok(Value::Float(ctx.rng.range_f64(fmin, fmax)))
            }
            Expr::RandomInt(min, max) => {
                let vmin = min.eval(ctx)?;
                let vmax = max.eval(ctx)?;
                let imin = vmin.as_int().ok_or_else(|| Error::TypeError {
                    expected: "int".to_string(),
                    got: vmin.type_name().to_string(),
                })?;
                let imax = vmax.as_int().ok_or_else(|| Error::TypeError {
                    expected: "int".to_string(),
                    got: vmax.type_name().to_string(),
                })?;
                Ok(Value::Int(ctx.rng.range_i64(imin, imax)))
            }
            Expr::WeightedRandom(weight_exprs) => {
                let mut weights = Vec::with_capacity(weight_exprs.len());
                for expr in weight_exprs {
                    let v = expr.eval(ctx)?;
                    let f = v.as_float().ok_or_else(|| Error::TypeError {
                        expected: "number".to_string(),
                        got: v.type_name().to_string(),
                    })?;
                    weights.push(f);
                }
                match ctx.rng.weighted_index(&weights) {
                    Some(i) => Ok(Value::Int(i as i64)),
                    None => Ok(Value::Null),
                }
            }

            // String
            Expr::Concat(exprs) => {
                let mut result = String::new();
                for expr in exprs {
                    let v = expr.eval(ctx)?;
                    result.push_str(&format!("{}", v));
                }
                Ok(Value::String(result))
            }
            Expr::Format(template, args) => {
                let mut result = template.clone();
                for (i, expr) in args.iter().enumerate() {
                    let v = expr.eval(ctx)?;
                    let placeholder = format!("{{{}}}", i);
                    result = result.replace(&placeholder, &format!("{}", v));
                }
                Ok(Value::String(result))
            }
        }
    }

    /// Create a literal expression
    pub fn lit(value: impl Into<Value>) -> Self {
        Expr::Literal(value.into())
    }

    /// Create a property access expression
    pub fn prop(name: impl Into<String>) -> Self {
        Expr::Property(name.into())
    }

    /// Create a global property access expression
    pub fn global(name: impl Into<String>) -> Self {
        Expr::Global(name.into())
    }

    /// Create a parameter access expression
    pub fn param(name: impl Into<String>) -> Self {
        Expr::Param(name.into())
    }
}

/// Helper to perform numeric operations
fn numeric_op(a: &Value, b: &Value, op: fn(f64, f64) -> f64) -> Result<Value> {
    let fa = a.as_float().ok_or_else(|| Error::TypeError {
        expected: "number".to_string(),
        got: a.type_name().to_string(),
    })?;
    let fb = b.as_float().ok_or_else(|| Error::TypeError {
        expected: "number".to_string(),
        got: b.type_name().to_string(),
    })?;
    Ok(Value::Float(op(fa, fb)))
}

/// Helper to compare values
fn compare_values(a: &Value, b: &Value, cmp: fn(f64, f64) -> bool) -> Result<Value> {
    let fa = a.as_float().ok_or_else(|| Error::TypeError {
        expected: "number".to_string(),
        got: a.type_name().to_string(),
    })?;
    let fb = b.as_float().ok_or_else(|| Error::TypeError {
        expected: "number".to_string(),
        got: b.type_name().to_string(),
    })?;
    Ok(Value::Bool(cmp(fa, fb)))
}

/// Check if two values are equal
fn values_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Null, Value::Null) => true,
        (Value::Bool(x), Value::Bool(y)) => x == y,
        (Value::Int(x), Value::Int(y)) => x == y,
        (Value::Float(x), Value::Float(y)) => (x - y).abs() < f64::EPSILON,
        (Value::Int(x), Value::Float(y)) | (Value::Float(y), Value::Int(x)) => {
            (*x as f64 - y).abs() < f64::EPSILON
        }
        (Value::String(x), Value::String(y)) => x == y,
        (Value::EntityRef(x), Value::EntityRef(y)) => x == y,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_context<'a>(
        entities: &'a EntityStore,
        globals: &'a ValueMap,
        params: &'a ValueMap,
        rng: &'a mut GameRng,
    ) -> EvalContext<'a> {
        EvalContext::new(entities, globals, params, rng)
    }

    #[test]
    fn test_literal() {
        let entities = EntityStore::new();
        let globals = ValueMap::new();
        let params = ValueMap::new();
        let mut rng = GameRng::new(42);
        let mut ctx = make_context(&entities, &globals, &params, &mut rng);

        let expr = Expr::lit(42i64);
        assert_eq!(expr.eval(&mut ctx).unwrap(), Value::Int(42));

        let expr = Expr::lit(3.14);
        assert_eq!(expr.eval(&mut ctx).unwrap(), Value::Float(3.14));
    }

    #[test]
    fn test_arithmetic() {
        let entities = EntityStore::new();
        let globals = ValueMap::new();
        let params = ValueMap::new();
        let mut rng = GameRng::new(42);
        let mut ctx = make_context(&entities, &globals, &params, &mut rng);

        let expr = Expr::Add(Box::new(Expr::lit(10.0)), Box::new(Expr::lit(5.0)));
        assert_eq!(expr.eval(&mut ctx).unwrap().as_float(), Some(15.0));

        let expr = Expr::Mul(Box::new(Expr::lit(3.0)), Box::new(Expr::lit(4.0)));
        assert_eq!(expr.eval(&mut ctx).unwrap().as_float(), Some(12.0));
    }

    #[test]
    fn test_comparison() {
        let entities = EntityStore::new();
        let globals = ValueMap::new();
        let params = ValueMap::new();
        let mut rng = GameRng::new(42);
        let mut ctx = make_context(&entities, &globals, &params, &mut rng);

        let expr = Expr::Gt(Box::new(Expr::lit(10.0)), Box::new(Expr::lit(5.0)));
        assert_eq!(expr.eval(&mut ctx).unwrap(), Value::Bool(true));

        let expr = Expr::Lt(Box::new(Expr::lit(10.0)), Box::new(Expr::lit(5.0)));
        assert_eq!(expr.eval(&mut ctx).unwrap(), Value::Bool(false));
    }

    #[test]
    fn test_logical() {
        let entities = EntityStore::new();
        let globals = ValueMap::new();
        let params = ValueMap::new();
        let mut rng = GameRng::new(42);
        let mut ctx = make_context(&entities, &globals, &params, &mut rng);

        let expr = Expr::And(vec![Expr::lit(true), Expr::lit(true)]);
        assert_eq!(expr.eval(&mut ctx).unwrap(), Value::Bool(true));

        let expr = Expr::And(vec![Expr::lit(true), Expr::lit(false)]);
        assert_eq!(expr.eval(&mut ctx).unwrap(), Value::Bool(false));

        let expr = Expr::Or(vec![Expr::lit(false), Expr::lit(true)]);
        assert_eq!(expr.eval(&mut ctx).unwrap(), Value::Bool(true));
    }

    #[test]
    fn test_property_access() {
        let mut entities = EntityStore::new();
        let entity = entities.create("nation");
        entity.set("gold", 100.0f64);
        let entity_id = entity.id;

        let globals = ValueMap::new();
        let params = ValueMap::new();
        let mut rng = GameRng::new(42);

        let entity = entities.get(entity_id).unwrap();
        let mut ctx = EvalContext::new(&entities, &globals, &params, &mut rng).with_target(entity);

        let expr = Expr::prop("gold");
        assert_eq!(expr.eval(&mut ctx).unwrap().as_float(), Some(100.0));
    }
}
