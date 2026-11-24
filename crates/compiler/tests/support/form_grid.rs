pub(crate) type Meters = (usize, usize, usize);

pub(crate) struct Axis {
    pub name: &'static str,
    pub forms: &'static [&'static str],
}

pub(crate) struct Cell<Outcome> {
    pub name: &'static str,
    pub at: &'static [&'static str],
    pub source: String,
    pub parent: Option<Meters>,
    pub exact: Meters,
    pub outcome: Outcome,
}

impl<Outcome> Cell<Outcome> {
    pub(crate) fn parent_still_holds(&self) -> Option<Meters> {
        self.parent.filter(|_| self.within_parent())
    }

    pub(crate) fn parent_now_exceeded(&self) -> Option<Meters> {
        self.parent.filter(|_| !self.within_parent())
    }

    fn within_parent(&self) -> bool {
        self.parent
            .is_some_and(|parent| self.exact.0 <= parent.0 && self.exact.1 <= parent.1)
    }
}

pub(crate) fn sweep<Outcome>(
    axes: &[Axis],
    cells: &[Cell<Outcome>],
    mut run: impl FnMut(&Cell<Outcome>),
) {
    for (index, cell) in cells.iter().enumerate() {
        assert_eq!(cell.at.len(), axes.len(), "{}", cell.name);
        for (axis, form) in axes.iter().zip(cell.at) {
            assert!(
                axis.forms.contains(form),
                "{}: `{form}` is not a form of axis {}",
                cell.name,
                axis.name
            );
        }
        for other in &cells[..index] {
            assert_ne!(
                other.at, cell.at,
                "{} and {} share a cell",
                other.name, cell.name
            );
            assert_ne!(other.name, cell.name, "two cells are named {}", cell.name);
        }
    }
    for cell in cells {
        run(cell);
    }
}
