use serde::{Deserialize, Serialize};
use smallvec::SmallVec;
use std::fmt;

#[derive(Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Coordinate {
    pub values: SmallVec<[usize; 4]>,
}

impl Coordinate {
    pub fn new<I: Into<SmallVec<[usize; 4]>>>(values: I) -> Self {
        Self {
            values: values.into(),
        }
    }

    pub fn dim(&self) -> usize {
        self.values.len()
    }
}

impl fmt::Debug for Coordinate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "(")?;
        for (i, v) in self.values.iter().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }
            write!(f, "{}", v)?;
        }
        write!(f, ")")
    }
}
