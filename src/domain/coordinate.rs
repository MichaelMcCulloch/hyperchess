use std::fmt;

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Coordinate {
    pub values: Vec<usize>,
}

impl Coordinate {
    pub fn new(values: Vec<usize>) -> Self {
        Self { values }
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
