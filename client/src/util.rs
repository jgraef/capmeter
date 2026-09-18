use std::{
    fmt::Display,
    pin::Pin,
    task::{Context, Poll},
};

use anyhow::Error;
use futures_util::Stream;
use pin_project_lite::pin_project;

pub trait IteratorExt {
    fn maybe_limit(self, limit: Option<usize>) -> MaybeLimit<Self>
    where
        Self: Sized;
}

impl<I> IteratorExt for I
where
    I: Iterator,
{
    fn maybe_limit(self, limit: Option<usize>) -> MaybeLimit<Self>
    where
        Self: Sized,
    {
        MaybeLimit { inner: self, limit }
    }
}

pub trait StreamExt {
    fn maybe_limit(self, limit: Option<usize>) -> MaybeLimit<Self>
    where
        Self: Sized;
}

impl<I> StreamExt for I
where
    I: Stream,
{
    fn maybe_limit(self, limit: Option<usize>) -> MaybeLimit<Self>
    where
        Self: Sized,
    {
        MaybeLimit { inner: self, limit }
    }
}

pin_project! {
    #[derive(Clone, Debug)]
    pub struct MaybeLimit<I> {
        #[pin]
        inner: I,
        limit: Option<usize>,
    }
}

impl<I> Iterator for MaybeLimit<I>
where
    I: Iterator,
{
    type Item = I::Item;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(limit) = &mut self.limit {
            if *limit == 0 {
                return None;
            }

            *limit -= 1;
        }

        self.inner.next()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let (mut lower, mut upper) = self.inner.size_hint();

        if let Some(limit) = self.limit {
            lower = lower.min(limit);

            if let Some(upper) = &mut upper {
                *upper = (*upper).min(limit);
            } else {
                upper = Some(limit);
            }
        }

        (lower, upper)
    }
}

impl<I> ExactSizeIterator for MaybeLimit<I> where I: ExactSizeIterator {}

impl<I> Stream for MaybeLimit<I>
where
    I: Stream,
{
    type Item = I::Item;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut this = self.project();

        if let Some(limit) = &mut this.limit {
            if *limit == 0 {
                return Poll::Ready(None);
            }

            let poll = this.inner.poll_next(cx);

            if poll.is_ready() {
                *limit -= 1;
            }

            poll
        } else {
            this.inner.poll_next(cx)
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let (mut lower, mut upper) = self.inner.size_hint();

        if let Some(limit) = self.limit {
            lower = lower.min(limit);

            if let Some(upper) = &mut upper {
                *upper = (*upper).min(limit);
            } else {
                upper = Some(limit);
            }
        }

        (lower, upper)
    }
}

pub const SI_PREFIXES: &[(i32, &'static str)] = &[
    (-15, "f"),
    (-12, "p"),
    (-9, "n"),
    (-6, "μ"),
    (-3, "m"),
    (0, ""),
    (3, "k"),
    (6, "M"),
    (9, "G"),
    (12, "T"),
    (15, "P"),
];

fn si_prefix(value: f64) -> Option<(i32, &'static str)> {
    let exponent = (value.log10() / 3.0).floor() as i32 * 3;
    SI_PREFIXES.iter().find(|(e, _p)| exponent == *e).copied()
}

pub fn format_si<'a>(value: f64, unit: &'a str) -> SiFormatter<'a> {
    if let Some((exponent, si_prefix)) = si_prefix(value) {
        SiFormatter {
            value: value / 10.0f64.powi(exponent),
            si_prefix,
            unit,
        }
    } else {
        SiFormatter {
            value,
            si_prefix: "",
            unit,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct SiFormatter<'a> {
    value: f64,
    si_prefix: &'static str,
    unit: &'a str,
}

impl<'a> Display for SiFormatter<'a> {
    fn fmt(&self, mut f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.value.fmt(&mut f)?;
        write!(f, " {}{}", self.si_prefix, self.unit)?;
        Ok(())
    }
}

pub fn parse_si(s: &str, unit: &str) -> Result<f64, Error> {
    let s = s.trim();
    let s = s.strip_suffix(unit).unwrap_or(s);

    for (exponent, si_prefix) in SI_PREFIXES {
        if let Some(s) = s.strip_suffix(si_prefix) {
            let x = s.parse::<f64>()?;
            return Ok(x * 10.0f64.powi(*exponent));
        }
    }

    todo!();
}

#[cfg(test)]
mod tests {
    use crate::util::si_prefix;

    #[test]
    fn it_determines_si_prefix() {
        assert_eq!(si_prefix(3.3e9), Some((9, "G")));
        assert_eq!(si_prefix(33.0e9), Some((9, "G")));
        assert_eq!(si_prefix(330.0e9), Some((9, "G")));
        assert_eq!(si_prefix(3.3e6), Some((6, "M")));
        assert_eq!(si_prefix(33.0e6), Some((6, "M")));
        assert_eq!(si_prefix(330.0e6), Some((6, "M")));
        assert_eq!(si_prefix(3.3e3), Some((3, "k")));
        assert_eq!(si_prefix(33.0e3), Some((3, "k")));
        assert_eq!(si_prefix(330.0e3), Some((3, "k")));

        assert_eq!(si_prefix(3.3), Some((0, "")));
        assert_eq!(si_prefix(33.0), Some((0, "")));
        assert_eq!(si_prefix(330.0), Some((0, "")));

        assert_eq!(si_prefix(3.3e-3), Some((-3, "m")));
        assert_eq!(si_prefix(33.0e-3), Some((-3, "m")));
        assert_eq!(si_prefix(330.0e-3), Some((-3, "m")));
        assert_eq!(si_prefix(3.3e-6), Some((-6, "μ")));
        assert_eq!(si_prefix(33.0e-6), Some((-6, "μ")));
        assert_eq!(si_prefix(330.0e-6), Some((-6, "μ")));
        assert_eq!(si_prefix(3.3e-9), Some((-9, "n")));
        assert_eq!(si_prefix(33.0e-9), Some((-9, "n")));
        assert_eq!(si_prefix(330.0e-9), Some((-9, "n")));
    }
}
