pub struct InterleaveAll<I, J>
where
    I: Iterator<Item = J>,
{
    index: usize,
    iterators: Vec<I>,
}

pub fn interleave_all<I, J, K>(i: I) -> InterleaveAll<<J as IntoIterator>::IntoIter, K>
where
    I: IntoIterator<Item = J>,
    J: IntoIterator<Item = K>,
{
    let iterators = i.into_iter().map(|element| element.into_iter()).collect();
    InterleaveAll {
        index: 0,
        iterators,
    }
}

impl<I, J> Iterator for InterleaveAll<I, J>
where
    I: Iterator<Item = J>,
{
    type Item = J;

    fn next(&mut self) -> Option<Self::Item> {
        let index = self.index;
        self.index = (self.index + 1) % self.iterators.len();

        self.iterators[index].next()
    }
}
