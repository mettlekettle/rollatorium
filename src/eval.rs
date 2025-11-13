use std::cmp::Ordering;
use std::collections::HashSet;

use rand::RngCore;
use rand::distr::{Distribution, Uniform};

use crate::Result;
use crate::ast::{
    Annotation, BinaryOperator, DiceSize, Node, Selector, SelectorKind, SetOperation, SetOperator,
    UnaryOperator,
};
use crate::error::RollatoriumError::Eval;

const EPSILON: f64 = 1e-9;

#[derive(Debug, Clone)]
pub struct EvalConfig {
    pub max_rolls: usize,
}

impl Default for EvalConfig {
    fn default() -> Self {
        Self { max_rolls: 1000 }
    }
}

#[derive(Debug, Clone)]
pub struct EvalResult {
    pub total: f64,
    pub value: Value,
}

#[derive(Debug, Clone)]
pub enum Value {
    Literal(f64),
    Unary {
        operator: UnaryOperator,
        operand: Box<EvalResult>,
    },
    Binary {
        operator: BinaryOperator,
        left: Box<EvalResult>,
        right: Box<EvalResult>,
    },
    Dice(DiceRoll),
    Set(SetRoll),
    Annotated {
        expr: Box<EvalResult>,
        annotations: Vec<Annotation>,
    },
}

#[derive(Debug, Clone)]
pub struct DiceRoll {
    pub quantity: usize,
    pub size: u32,
    pub dice: Vec<DieResult>,
    pub operations: Vec<SetOperation>,
}

#[derive(Debug, Clone)]
pub struct DieResult {
    pub value: f64,
    pub rolls: Vec<f64>,
    pub kept: bool,
    pub dropped: bool,
    pub origin: DieOrigin,
    pub adjustments: Vec<DieAdjustment>,
}

impl DieResult {
    fn new(value: f64, origin: DieOrigin) -> Self {
        Self {
            value,
            rolls: vec![value],
            kept: true,
            dropped: false,
            origin,
            adjustments: Vec::new(),
        }
    }

    fn refresh_drop_state(&mut self) {
        self.dropped = !self.kept;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DieOrigin {
    Original,
    RerollAdd,
    Explosion,
}

#[derive(Debug, Clone)]
pub enum DieAdjustment {
    Minimum { threshold: f64, previous: f64 },
    Maximum { threshold: f64, previous: f64 },
}

#[derive(Debug, Clone)]
pub struct SetRoll {
    pub elements: Vec<SetElement>,
    pub operations: Vec<SetOperation>,
}

#[derive(Debug, Clone)]
pub struct SetElement {
    pub value: EvalResult,
    pub kept: bool,
    pub dropped: bool,
}

impl SetElement {
    fn refresh_drop_state(&mut self) {
        self.dropped = !self.kept;
    }
}

pub fn evaluate(expr: &Node) -> Result<EvalResult> {
    evaluate_with_config(expr, EvalConfig::default())
}

pub fn evaluate_with_config(expr: &Node, config: EvalConfig) -> Result<EvalResult> {
    evaluate_with_rng(expr, config, rand::rng())
}

pub fn evaluate_with_rng<R>(expr: &Node, config: EvalConfig, rng: R) -> Result<EvalResult>
where
    R: RngCore,
{
    Evaluator {
        rng,
        config,
        rolls: 0,
    }
    .eval(expr)
}

struct Evaluator<R: RngCore> {
    rng: R,
    config: EvalConfig,
    rolls: usize,
}

impl<R: RngCore> Evaluator<R> {
    fn eval(&mut self, node: &Node) -> Result<EvalResult> {
        match node {
            Node::Literal(v) => Ok(EvalResult {
                total: *v,
                value: Value::Literal(*v),
            }),
            Node::Unary { operator, operand } => {
                let evaluated = self.eval(operand)?;
                let total = match operator {
                    UnaryOperator::Plus => evaluated.total,
                    UnaryOperator::Minus => -evaluated.total,
                };
                Ok(EvalResult {
                    total,
                    value: Value::Unary {
                        operator: *operator,
                        operand: Box::new(evaluated),
                    },
                })
            }
            Node::Binary {
                operator,
                left,
                right,
            } => {
                let left_eval = self.eval(left)?;
                let right_eval = self.eval(right)?;
                let total = match operator {
                    BinaryOperator::Add => left_eval.total + right_eval.total,
                    BinaryOperator::Subtract => left_eval.total - right_eval.total,
                    BinaryOperator::Multiply => left_eval.total * right_eval.total,
                    BinaryOperator::Divide => left_eval.total / right_eval.total,
                    BinaryOperator::IntDivide => (left_eval.total / right_eval.total).trunc(),
                    BinaryOperator::Modulo => left_eval.total % right_eval.total,
                    BinaryOperator::Equal => (left_eval.total == right_eval.total) as i32 as f64,
                    BinaryOperator::NotEqual => (left_eval.total != right_eval.total) as i32 as f64,
                    BinaryOperator::Greater => (left_eval.total > right_eval.total) as i32 as f64,
                    BinaryOperator::GreaterEqual => {
                        (left_eval.total >= right_eval.total) as i32 as f64
                    }
                    BinaryOperator::Less => (left_eval.total < right_eval.total) as i32 as f64,
                    BinaryOperator::LessEqual => {
                        (left_eval.total <= right_eval.total) as i32 as f64
                    }
                };
                Ok(EvalResult {
                    total,
                    value: Value::Binary {
                        operator: *operator,
                        left: Box::new(left_eval),
                        right: Box::new(right_eval),
                    },
                })
            }
            Node::Dice { num, size } => self.eval_dice(num.as_deref(), size, &[]),
            Node::DiceWithOps { dice, operations } => match dice.as_ref() {
                Node::Dice { num, size } => self.eval_dice(num.as_deref(), size, operations),
                other => Err(Eval(format!(
                    "DiceWithOps must contain a dice node, found {:?}",
                    other
                ))),
            },
            Node::Set {
                elements,
                operations,
            } => self.eval_set(elements, operations),
            Node::Annotated { expr, annotations } => {
                let evaluated = self.eval(expr)?;
                Ok(EvalResult {
                    total: evaluated.total,
                    value: Value::Annotated {
                        expr: Box::new(evaluated),
                        annotations: annotations.clone(),
                    },
                })
            }
        }
    }

    fn eval_dice(
        &mut self,
        quantity: Option<&Node>,
        size: &DiceSize,
        operations: &[SetOperation],
    ) -> Result<EvalResult> {
        let quantity_value = match quantity {
            Some(node) => {
                let result = self.eval(node)?;
                self.as_usize(result.total, "dice quantity")?
            }
            None => 1,
        };

        let (die_low, die_high) = match size {
            DiceSize::Percent => (0u32, 9),
            DiceSize::Value(inner) => {
                let result = self.eval(inner)?;
                (1, self.as_u32(result.total, "die size")?)
            }
        };

        if die_high == 0 {
            return Err(Eval("Die size must be positive".into()));
        }

        let distribution = Uniform::new_inclusive(die_low, die_high)
            .map_err(|err| Eval(format!("Invalid die size {}: {}", die_high, err)))?;
        let mut dice = Vec::with_capacity(quantity_value);
        for _ in 0..quantity_value {
            let roll = self.roll_die(&distribution, size)?;
            dice.push(DieResult::new(roll, DieOrigin::Original));
        }

        self.apply_dice_operations(&mut dice, &distribution, operations, size)?;
        for die in &mut dice {
            die.refresh_drop_state();
        }
        let total: f64 = dice.iter().filter(|d| d.kept).map(|d| d.value).sum();
        Ok(EvalResult {
            total,
            value: Value::Dice(DiceRoll {
                quantity: quantity_value,
                size: die_high,
                dice,
                operations: operations.to_vec(),
            }),
        })
    }

    fn eval_set(&mut self, elements: &[Node], operations: &[SetOperation]) -> Result<EvalResult> {
        let mut evaluated_elements = Vec::with_capacity(elements.len());
        for element in elements {
            let value = self.eval(element)?;
            evaluated_elements.push(SetElement {
                value,
                kept: true,
                dropped: false,
            });
        }

        self.apply_set_operations(&mut evaluated_elements, operations)?;
        for element in &mut evaluated_elements {
            element.refresh_drop_state();
        }
        let total: f64 = evaluated_elements
            .iter()
            .filter(|e| e.kept)
            .map(|e| e.value.total)
            .sum();
        Ok(EvalResult {
            total,
            value: Value::Set(SetRoll {
                elements: evaluated_elements,
                operations: operations.to_vec(),
            }),
        })
    }

    fn roll_die(&mut self, distribution: &Uniform<u32>, die_size: &DiceSize) -> Result<f64> {
        if self.rolls >= self.config.max_rolls {
            return Err(Eval("Exceeded maximum number of rolls".into()));
        }
        self.rolls += 1;
        let mut value = distribution.sample(&mut self.rng) as f64;
        if DiceSize::Percent == *die_size {
            value *= 10.0;
        }

        Ok(value)
    }

    fn as_usize(&self, value: f64, context: &str) -> Result<usize> {
        if value < 0.0 {
            return Err(Eval(format!("{} must be non-negative", context)));
        }
        if (value.round() - value).abs() > EPSILON {
            return Err(Eval(format!(
                "{} must be an integer, found {}",
                context, value
            )));
        }
        Ok(value.round() as usize)
    }

    fn as_u32(&self, value: f64, context: &str) -> Result<u32> {
        if value <= 0.0 {
            return Err(Eval(format!("{} must be positive", context)));
        }
        if (value.round() - value).abs() > EPSILON {
            return Err(Eval(format!(
                "{} must be an integer, found {}",
                context, value
            )));
        }
        Ok(value.round() as u32)
    }

    fn apply_dice_operations(
        &mut self,
        dice: &mut Vec<DieResult>,
        distribution: &Uniform<u32>,
        operations: &[SetOperation],
        size: &DiceSize,
    ) -> Result<()> {
        for operation in operations {
            match operation.operator {
                SetOperator::Keep => {
                    let selected = self.select_dice(dice, &operation.selectors)?;
                    let selected: HashSet<_> = selected.into_iter().collect();
                    for (idx, die) in dice.iter_mut().enumerate() {
                        if die.kept {
                            die.kept = selected.contains(&idx);
                        }
                    }
                }
                SetOperator::Drop => {
                    let selected = self.select_dice(dice, &operation.selectors)?;
                    for idx in selected {
                        if let Some(die) = dice.get_mut(idx) {
                            die.kept = false;
                        }
                    }
                }
                SetOperator::Reroll => loop {
                    let selected = self.select_dice(dice, &operation.selectors)?;
                    if selected.is_empty() {
                        break;
                    }
                    let mut changed = false;
                    for idx in selected {
                        if let Some(die) = dice.get_mut(idx) {
                            let new_value = self.roll_die(distribution, size)?;
                            die.rolls.push(new_value);
                            die.value = new_value;
                            changed = true;
                        }
                    }
                    if !changed {
                        break;
                    }
                },
                SetOperator::RerollOnce => {
                    let selected = self.select_dice(dice, &operation.selectors)?;
                    for idx in selected {
                        if let Some(die) = dice.get_mut(idx) {
                            let new_value = self.roll_die(distribution, size)?;
                            die.rolls.push(new_value);
                            die.value = new_value;
                        }
                    }
                }
                SetOperator::RerollAdd => {
                    let selected = self.select_dice(dice, &operation.selectors)?;
                    for _ in 0..selected.len() {
                        let new_value = self.roll_die(distribution, size)?;
                        dice.push(DieResult::new(new_value, DieOrigin::RerollAdd));
                    }
                }
                SetOperator::Explode => {
                    let mut queue = self.select_dice(dice, &operation.selectors)?;
                    let mut idx = 0;
                    while idx < queue.len() {
                        idx += 1;
                        let new_value = self.roll_die(distribution, size)?;
                        dice.push(DieResult::new(new_value, DieOrigin::Explosion));
                        let new_idx = dice.len() - 1;
                        let matches = self
                            .select_dice(dice, &operation.selectors)?
                            .into_iter()
                            .any(|i| i == new_idx);
                        if matches {
                            queue.push(new_idx);
                        }
                    }
                }
                SetOperator::Minimum => {
                    if operation.selectors.is_empty() {
                        return Err(Eval("Minimum operation requires a selector".into()));
                    }
                    if operation.selectors[0].kind != SelectorKind::Literal {
                        return Err(Eval("selector target must be positive".into()));
                    }
                    let threshold = self.eval(&operation.selectors[0].target)?.total;
                    if threshold <= 0.0 {
                        return Err(Eval("selector target must be positive".into()));
                    }
                    let affected = if operation.selectors.len() > 1 {
                        self.select_dice(dice, &operation.selectors[1..])?
                    } else {
                        dice.iter()
                            .enumerate()
                            .filter(|(_, die)| die.kept)
                            .map(|(idx, _)| idx)
                            .collect()
                    };
                    for idx in affected {
                        if let Some(die) = dice.get_mut(idx)
                            && die.value < threshold
                        {
                            let previous = die.value;
                            die.value = threshold;
                            die.adjustments.push(DieAdjustment::Minimum {
                                threshold,
                                previous,
                            });
                        }
                    }
                }
                SetOperator::Maximum => {
                    if operation.selectors.is_empty() {
                        return Err(Eval("Maximum operation requires a selector".into()));
                    }
                    if operation.selectors[0].kind != SelectorKind::Literal {
                        return Err(Eval("selector target must be positive".into()));
                    }
                    let threshold = self.eval(&operation.selectors[0].target)?.total;
                    let affected = if operation.selectors.len() > 1 {
                        self.select_dice(dice, &operation.selectors[1..])?
                    } else {
                        dice.iter()
                            .enumerate()
                            .filter(|(_, die)| die.kept)
                            .map(|(idx, _)| idx)
                            .collect()
                    };
                    for idx in affected {
                        if let Some(die) = dice.get_mut(idx)
                            && die.value > threshold
                        {
                            let previous = die.value;
                            die.value = threshold;
                            die.adjustments.push(DieAdjustment::Maximum {
                                threshold,
                                previous,
                            });
                        }
                    }
                }
                other => {
                    return Err(Eval(format!(
                        "Set operation {:?} is not supported in the evaluator",
                        other
                    )));
                }
            }
        }
        Ok(())
    }

    fn apply_set_operations(
        &mut self,
        elements: &mut [SetElement],
        operations: &[SetOperation],
    ) -> Result<()> {
        let mut keep_initialized = false;
        for operation in operations {
            match operation.operator {
                SetOperator::Keep => {
                    let selected =
                        self.select_set_elements(elements, &operation.selectors, false)?;
                    if !keep_initialized {
                        for element in elements.iter_mut() {
                            element.kept = false;
                        }
                        keep_initialized = true;
                    }
                    for idx in selected {
                        if let Some(element) = elements.get_mut(idx) {
                            element.kept = true;
                        }
                    }
                }
                SetOperator::Drop => {
                    let selected =
                        self.select_set_elements(elements, &operation.selectors, true)?;
                    for idx in selected {
                        if let Some(element) = elements.get_mut(idx) {
                            element.kept = false;
                        }
                    }
                }
                other => {
                    return Err(Eval(format!(
                        "Set operation {:?} is not supported for sets",
                        other
                    )));
                }
            }
        }
        Ok(())
    }

    fn select_dice(&mut self, dice: &[DieResult], selectors: &[Selector]) -> Result<Vec<usize>> {
        if selectors.is_empty() {
            return Ok(Vec::new());
        }
        let mut selected = HashSet::new();
        for selector in selectors {
            let mut indices = match selector.kind {
                SelectorKind::Highest => {
                    let value = self.eval(&selector.target)?.total;
                    let count = self.as_usize(value, "selector")?;
                    self.select_highest(dice, count)
                }
                SelectorKind::Lowest => {
                    let value = self.eval(&selector.target)?.total;
                    let count = self.as_usize(value, "selector")?;
                    self.select_lowest(dice, count)
                }
                SelectorKind::GreaterThan => {
                    let value = self.eval(&selector.target)?.total;
                    self.select_value(dice, |die_value| die_value > value)
                }
                SelectorKind::GreaterThanOrEqual => {
                    let value = self.eval(&selector.target)?.total;
                    self.select_value(dice, |die_value| die_value >= value)
                }
                SelectorKind::LessThan => {
                    let value = self.eval(&selector.target)?.total;
                    self.select_value(dice, |die_value| die_value < value)
                }
                SelectorKind::LessThanOrEqual => {
                    let value = self.eval(&selector.target)?.total;
                    self.select_value(dice, |die_value| die_value <= value)
                }
                SelectorKind::EqualTo => {
                    let value = self.eval(&selector.target)?.total;
                    self.select_value(dice, |die_value| (die_value - value).abs() <= EPSILON)
                }
                SelectorKind::NotEqual => {
                    let value = self.eval(&selector.target)?.total;
                    self.select_value(dice, |die_value| (die_value - value).abs() > EPSILON)
                }
                SelectorKind::Literal => {
                    let value = self.eval(&selector.target)?.total;
                    self.select_value(dice, |die_value| (die_value - value).abs() <= EPSILON)
                }
            }?;
            selected.extend(indices.drain(..));
        }
        let mut collected: Vec<_> = selected.into_iter().collect();
        collected.sort_unstable();
        Ok(collected)
    }

    fn select_set_elements(
        &mut self,
        elements: &[SetElement],
        selectors: &[Selector],
        only_kept: bool,
    ) -> Result<Vec<usize>> {
        if selectors.is_empty() {
            return Ok(Vec::new());
        }
        let mut selected = HashSet::new();
        for selector in selectors {
            let mut indices = match selector.kind {
                SelectorKind::Highest => {
                    let value = self.eval(&selector.target)?.total;
                    let count = self.as_usize(value, "selector")?;
                    self.select_set_highest(elements, count, only_kept)
                }
                SelectorKind::Lowest => {
                    let value = self.eval(&selector.target)?.total;
                    let count = self.as_usize(value, "selector")?;
                    self.select_set_lowest(elements, count, only_kept)
                }
                SelectorKind::GreaterThan => {
                    let value = self.eval(&selector.target)?.total;
                    self.select_set_value(elements, |element| element > value, only_kept)
                }
                SelectorKind::GreaterThanOrEqual => {
                    let value = self.eval(&selector.target)?.total;
                    self.select_set_value(elements, |element| element >= value, only_kept)
                }
                SelectorKind::LessThan => {
                    let value = self.eval(&selector.target)?.total;
                    self.select_set_value(elements, |element| element < value, only_kept)
                }
                SelectorKind::LessThanOrEqual => {
                    let value = self.eval(&selector.target)?.total;
                    self.select_set_value(elements, |element| element <= value, only_kept)
                }
                SelectorKind::EqualTo => {
                    let value = self.eval(&selector.target)?.total;
                    self.select_set_value(
                        elements,
                        |element| (element - value).abs() <= EPSILON,
                        only_kept,
                    )
                }
                SelectorKind::NotEqual => {
                    let value = self.eval(&selector.target)?.total;
                    self.select_set_value(
                        elements,
                        |element| (element - value).abs() > EPSILON,
                        only_kept,
                    )
                }
                SelectorKind::Literal => {
                    let value = self.eval(&selector.target)?.total;
                    self.select_set_value(
                        elements,
                        |element| (element - value).abs() <= EPSILON,
                        only_kept,
                    )
                }
            }?;
            selected.extend(indices.drain(..));
        }
        let mut collected: Vec<_> = selected.into_iter().collect();
        collected.sort_unstable();
        Ok(collected)
    }

    fn select_highest(&self, dice: &[DieResult], count: usize) -> Result<Vec<usize>> {
        let mut indices: Vec<_> = dice
            .iter()
            .enumerate()
            .filter(|(_, die)| die.kept)
            .map(|(idx, _)| idx)
            .collect();
        indices.sort_by(|a, b| self.compare_desc(&dice[*a].value, &dice[*b].value));
        indices.truncate(count.min(indices.len()));
        Ok(indices)
    }

    fn select_lowest(&self, dice: &[DieResult], count: usize) -> Result<Vec<usize>> {
        let mut indices: Vec<_> = dice
            .iter()
            .enumerate()
            .filter(|(_, die)| die.kept)
            .map(|(idx, _)| idx)
            .collect();
        indices.sort_by(|a, b| self.compare_asc(&dice[*a].value, &dice[*b].value));
        indices.truncate(count.min(indices.len()));
        Ok(indices)
    }

    fn select_value<F>(&self, dice: &[DieResult], predicate: F) -> Result<Vec<usize>>
    where
        F: Fn(f64) -> bool,
    {
        Ok(dice
            .iter()
            .enumerate()
            .filter(|(_, die)| die.kept && predicate(die.value))
            .map(|(idx, _)| idx)
            .collect())
    }

    fn select_set_highest(
        &self,
        elements: &[SetElement],
        count: usize,
        only_kept: bool,
    ) -> Result<Vec<usize>> {
        let mut indices: Vec<_> = elements
            .iter()
            .enumerate()
            .filter(|(_, element)| !only_kept || element.kept)
            .map(|(idx, _)| idx)
            .collect();
        indices.sort_by(|a, b| {
            self.compare_desc(&elements[*a].value.total, &elements[*b].value.total)
        });
        indices.truncate(count.min(indices.len()));
        Ok(indices)
    }

    fn select_set_lowest(
        &self,
        elements: &[SetElement],
        count: usize,
        only_kept: bool,
    ) -> Result<Vec<usize>> {
        let mut indices: Vec<_> = elements
            .iter()
            .enumerate()
            .filter(|(_, element)| !only_kept || element.kept)
            .map(|(idx, _)| idx)
            .collect();
        indices
            .sort_by(|a, b| self.compare_asc(&elements[*a].value.total, &elements[*b].value.total));
        indices.truncate(count.min(indices.len()));
        Ok(indices)
    }

    fn select_set_value<F>(
        &self,
        elements: &[SetElement],
        predicate: F,
        only_kept: bool,
    ) -> Result<Vec<usize>>
    where
        F: Fn(f64) -> bool,
    {
        Ok(elements
            .iter()
            .enumerate()
            .filter(|(_, element)| (!only_kept || element.kept) && predicate(element.value.total))
            .map(|(idx, _)| idx)
            .collect())
    }

    fn compare_desc(&self, a: &f64, b: &f64) -> Ordering {
        b.partial_cmp(a).unwrap_or(Ordering::Equal)
    }

    fn compare_asc(&self, a: &f64, b: &f64) -> Ordering {
        a.partial_cmp(b).unwrap_or(Ordering::Equal)
    }
}
