//! Token usage from transcripts, and what it cost.
//!
//! One API response is written as several `assistant` lines — one per content
//! block — and every one of them repeats the response's full `usage`. Summing
//! per line counts a response once per block. So usage is keyed by response
//! first (`message.id`, else `requestId`) and only then summed.
//!
//! Money is integer micro-dollars throughout. Prices are parsed from decimal
//! text without passing through a float.

use std::collections::BTreeMap;
use std::iter::Sum;
use std::ops::{Add, AddAssign};

use serde_json::Value;

use crate::ingest::json::{str_at, u64_at};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Usage {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write: u64,
}

impl Usage {
    /// A `message.usage` object. Absent counters are zero; a `usage` that is
    /// not an object is no usage at all.
    pub fn from_json(u: &Value) -> Option<Usage> {
        if !u.is_object() {
            return None;
        }
        let n = |key| u64_at(u, key).unwrap_or(0);
        Some(Usage {
            input: n("input_tokens"),
            output: n("output_tokens"),
            cache_read: n("cache_read_input_tokens"),
            cache_write: n("cache_creation_input_tokens"),
        })
    }

    pub fn total(&self) -> u64 {
        self.input + self.output + self.cache_read + self.cache_write
    }
}

impl Add for Usage {
    type Output = Usage;
    fn add(self, rhs: Usage) -> Usage {
        Usage {
            input: self.input + rhs.input,
            output: self.output + rhs.output,
            cache_read: self.cache_read + rhs.cache_read,
            cache_write: self.cache_write + rhs.cache_write,
        }
    }
}

impl AddAssign for Usage {
    fn add_assign(&mut self, rhs: Usage) {
        *self = *self + rhs;
    }
}

impl Sum for Usage {
    fn sum<I: Iterator<Item = Usage>>(iter: I) -> Usage {
        iter.fold(Usage::default(), Add::add)
    }
}

/// One API response's usage, as read off any one of its lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Response<'a> {
    pub key: &'a str,
    pub model: &'a str,
    pub usage: Usage,
}

/// The usage an `assistant` line reports, keyed by the response it belongs
/// to. `None` for any other line, and for synthetic error lines whose model
/// is `<synthetic>`.
pub fn response(line: &Value) -> Option<Response<'_>> {
    if str_at(line, "type") != Some("assistant") {
        return None;
    }
    let message = line.get("message")?;
    let model = str_at(message, "model").filter(|m| *m != "<synthetic>")?;
    let key = str_at(message, "id").or_else(|| str_at(line, "requestId"))?;
    let usage = message.get("usage").and_then(Usage::from_json)?;
    Some(Response { key, model, usage })
}

/// Total usage per model, counting each response once however many lines it
/// was split across. The last line of a response wins.
///
/// The keys of the intermediate map borrow from the lines, which is why this
/// takes `&'a Value`s and not lines it would have to parse itself.
pub fn usage_by_model<'a, I>(lines: I) -> BTreeMap<&'a str, Usage>
where
    I: IntoIterator<Item = &'a Value>,
{
    let responses: BTreeMap<&str, Response<'_>> = lines
        .into_iter()
        .filter_map(response)
        .map(|r| (r.key, r))
        .collect();

    let mut by_model: BTreeMap<&str, Usage> = BTreeMap::new();
    for r in responses.into_values() {
        *by_model.entry(r.model).or_default() += r.usage;
    }
    by_model
}

/// Micro-dollars per million tokens, per bucket.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rates {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write: u64,
}

impl Rates {
    /// `usage` priced at these rates, in micro-dollars, rounded down.
    pub fn cost_micros(&self, usage: &Usage) -> u64 {
        let bucket = |tokens: u64, rate: u64| u128::from(tokens) * u128::from(rate);
        let total = bucket(usage.input, self.input)
            + bucket(usage.output, self.output)
            + bucket(usage.cache_read, self.cache_read)
            + bucket(usage.cache_write, self.cache_write);
        u64::try_from(total / 1_000_000).unwrap_or(u64::MAX)
    }
}

/// A price as the pricing page writes it: `$6.25 / MTok`, `$0.50`, `5`.
/// At most six decimal places. Returns micro-dollars.
pub fn parse_price(text: &str) -> Option<u64> {
    let amount = text
        .split('/')
        .next()?
        .trim()
        .trim_start_matches('$')
        .trim();
    let (whole, frac) = amount.split_once('.').unwrap_or((amount, ""));
    if whole.is_empty() || frac.len() > 6 {
        return None;
    }
    if !whole
        .bytes()
        .chain(frac.bytes())
        .all(|b| b.is_ascii_digit())
    {
        return None;
    }
    let whole: u64 = whole.parse().ok()?;
    let frac_micros: u64 = if frac.is_empty() {
        0
    } else {
        frac.parse::<u64>().ok()? * 10u64.pow(6 - frac.len() as u32)
    };
    whole.checked_mul(1_000_000)?.checked_add(frac_micros)
}

/// Look a model up by exact id, then with a bracketed context suffix such as
/// `[1m]` removed.
pub fn rates_for<'t>(table: &'t BTreeMap<String, Rates>, model: &str) -> Option<&'t Rates> {
    table
        .get(model)
        .or_else(|| model.split_once('[').and_then(|(base, _)| table.get(base)))
}

/// The cost of a whole transcript, and the models it could not price.
pub fn transcript_cost<'a, I>(lines: I, table: &BTreeMap<String, Rates>) -> (u64, Vec<&'a str>)
where
    I: IntoIterator<Item = &'a Value>,
{
    let (priced, unpriced): (Vec<_>, Vec<_>) = usage_by_model(lines)
        .into_iter()
        .map(|(model, usage)| {
            (
                model,
                rates_for(table, model).map(|r| r.cost_micros(&usage)),
            )
        })
        .partition(|(_, cost)| cost.is_some());
    let total = priced.iter().filter_map(|(_, c)| *c).sum();
    (total, unpriced.into_iter().map(|(m, _)| m).collect())
}
