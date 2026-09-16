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
        todo!("T07")
    }

    pub fn total(&self) -> u64 {
        todo!("T07")
    }
}

impl Add for Usage {
    type Output = Usage;
    fn add(self, rhs: Usage) -> Usage {
        todo!("T07")
    }
}

impl AddAssign for Usage {
    fn add_assign(&mut self, rhs: Usage) {
        todo!("T07")
    }
}

impl Sum for Usage {
    fn sum<I: Iterator<Item = Usage>>(iter: I) -> Usage {
        todo!("T07")
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
    todo!("T07")
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
    todo!("T07")
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
        todo!("T07 stretch")
    }
}

/// A price as the pricing page writes it: `$6.25 / MTok`, `$0.50`, `5`.
/// At most six decimal places. Returns micro-dollars.
pub fn parse_price(text: &str) -> Option<u64> {
    todo!("T07 stretch")
}

/// Look a model up by exact id, then with a bracketed context suffix such as
/// `[1m]` removed.
pub fn rates_for<'t>(table: &'t BTreeMap<String, Rates>, model: &str) -> Option<&'t Rates> {
    todo!("T07 stretch")
}

/// The cost of a whole transcript, and the models it could not price.
pub fn transcript_cost<'a, I>(lines: I, table: &BTreeMap<String, Rates>) -> (u64, Vec<&'a str>)
where
    I: IntoIterator<Item = &'a Value>,
{
    todo!("T07 stretch")
}
