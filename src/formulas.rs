//! Ready-made formulas, by subject, from `locales/formulas.txt`.

use std::sync::OnceLock;

use crate::bib::fold;
use crate::i18n::{Lang, current};

#[derive(Debug)]
pub struct Formula {
    pub category: usize,
    names: [String; 2],
    /// The inside of the `$ ... $`.
    pub math: String,
    haystack: String,
}

impl Formula {
    pub fn name(&self) -> &str {
        self.names[language_index()].as_str()
    }

    /// As it goes in a document: a block of its own.
    pub fn code(&self) -> String {
        format!("$ {} $", self.math)
    }

    pub fn matches(&self, query: &str) -> bool {
        fold(query).split_whitespace().all(|word| self.haystack.contains(word))
    }
}

#[derive(Debug, Default)]
pub struct Catalog {
    categories: Vec<[String; 2]>,
    pub formulas: Vec<Formula>,
}

impl Catalog {
    pub fn category(&self, index: usize) -> &str {
        self.categories.get(index).map_or("", |names| names[language_index()].as_str())
    }

    pub fn category_count(&self) -> usize {
        self.categories.len()
    }
}

fn language_index() -> usize {
    usize::from(current() == Lang::En)
}

fn parse(text: &str) -> Catalog {
    let mut catalog = Catalog::default();
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("# ") {
            // The file's own note has no second name.
            if let Some((pt, en)) = rest.split_once(" | ") {
                catalog.categories.push([pt.trim().to_string(), en.trim().to_string()]);
            }
        } else {
            // The math may hold bars of its own, so only the first two split.
            let mut parts = line.splitn(3, " | ");
            if let (Some(pt), Some(en), Some(math)) = (parts.next(), parts.next(), parts.next()) {
                let category = catalog.categories.len().saturating_sub(1);
                let subject = catalog.categories.last().map_or_else(String::new, |names| names.join(" "));
                let haystack = fold(&format!("{pt} {en} {subject} {math}"));
                catalog.formulas.push(Formula {
                    category,
                    names: [pt.trim().to_string(), en.trim().to_string()],
                    math: math.trim().to_string(),
                    haystack,
                });
            }
        }
    }
    catalog
}

pub fn catalog() -> &'static Catalog {
    static CATALOG: OnceLock<Catalog> = OnceLock::new();
    CATALOG.get_or_init(|| parse(include_str!("../locales/formulas.txt")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn the_catalog_is_whole() {
        let all = catalog();
        assert!(all.formulas.len() >= 90, "{}", all.formulas.len());
        assert_eq!(all.category_count(), 10);
        let names: HashSet<&String> = all.formulas.iter().map(|f| &f.names[0]).collect();
        assert_eq!(names.len(), all.formulas.len(), "two formulas with one name");
        for formula in &all.formulas {
            assert!(!formula.names[1].is_empty() && !formula.math.is_empty(), "{:?}", formula.names);
            assert!(formula.category < all.category_count());
        }
    }

    #[test]
    fn a_search_finds_a_formula_by_any_of_its_names_or_its_subject() {
        let find = |query: &str| catalog().formulas.iter().filter(|f| f.matches(query)).map(|f| f.names[1].as_str()).collect::<Vec<_>>();
        assert!(find("bhaskara").contains(&"Quadratic formula"));
        assert!(find("quadratic").contains(&"Quadratic formula"));
        assert!(find("variancia").contains(&"Sample variance"));
        assert!(find("estatistica desvio").contains(&"Sample standard deviation"));
        assert!(find("xyzzy").is_empty());
    }
}
