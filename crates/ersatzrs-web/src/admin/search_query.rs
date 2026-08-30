//! Serializable projection of the core-owned search AST for the admin query composer.
//!
//! Parsing and canonical formatting stay in `ersatzrs-core`. This module is the
//! transport/edit model shared by SSR and the WASM island; it is never a second
//! query parser.

use serde::{Deserialize, Serialize};

pub const SEARCH_QUERY_REQUIRED_CODE: &str = "search_query.required";
pub const SEARCH_QUERY_RULE_REQUIRED_CODE: &str = "search_query.rule_required";
pub const SEARCH_QUERY_GROUP_REQUIRED_CODE: &str = "search_query.group_required";
pub const SEARCH_QUERY_WILDCARD_REQUIRED_CODE: &str = "search_query.wildcard_required";

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct QueryComposerModel {
    pub all: Vec<QueryComposerNode>,
    pub any: Vec<QueryComposerNode>,
    pub none: Vec<QueryComposerNode>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum QueryComposerNode {
    Rule(QueryComposerRule),
    Group(QueryComposerGroup),
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct QueryComposerGroup {
    pub kind: QueryGroupKind,
    pub children: Vec<QueryComposerNode>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum QueryGroupKind {
    All,
    Any,
    None,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct QueryComposerRule {
    pub field: String,
    pub value: QueryRuleValue,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum QueryRuleValue {
    Text(String),
    Wildcard(String),
    Fuzzy {
        value: String,
        max_edits: usize,
    },
    Phrase {
        value: String,
        slop: usize,
    },
    Range {
        start: Option<String>,
        end: Option<String>,
        inclusive: bool,
    },
}

impl Default for QueryComposerRule {
    fn default() -> Self {
        Self {
            field: "title".to_owned(),
            value: QueryRuleValue::Text(String::new()),
        }
    }
}

impl QueryComposerNode {
    #[must_use]
    pub fn is_nested(&self) -> bool {
        matches!(self, Self::Group(_))
    }
}

impl QueryComposerModel {
    #[must_use]
    pub fn nodes(&self, kind: QueryGroupKind) -> &[QueryComposerNode] {
        match kind {
            QueryGroupKind::All => &self.all,
            QueryGroupKind::Any => &self.any,
            QueryGroupKind::None => &self.none,
        }
    }

    pub fn nodes_mut(&mut self, kind: QueryGroupKind) -> &mut Vec<QueryComposerNode> {
        match kind {
            QueryGroupKind::All => &mut self.all,
            QueryGroupKind::Any => &mut self.any,
            QueryGroupKind::None => &mut self.none,
        }
    }

    #[must_use]
    pub fn nodes_at_path(
        &self,
        kind: QueryGroupKind,
        path: &[usize],
    ) -> Option<&[QueryComposerNode]> {
        nested_nodes(self.nodes(kind), path)
    }

    pub fn nodes_at_path_mut(
        &mut self,
        kind: QueryGroupKind,
        path: &[usize],
    ) -> Option<&mut Vec<QueryComposerNode>> {
        nested_nodes_mut(self.nodes_mut(kind), path)
    }

    #[must_use]
    pub fn group_at_path(
        &self,
        kind: QueryGroupKind,
        path: &[usize],
    ) -> Option<&QueryComposerGroup> {
        let (index, parent_path) = path.split_last()?;
        match self.nodes_at_path(kind, parent_path)?.get(*index)? {
            QueryComposerNode::Group(group) => Some(group),
            QueryComposerNode::Rule(_) => None,
        }
    }

    pub fn group_at_path_mut(
        &mut self,
        kind: QueryGroupKind,
        path: &[usize],
    ) -> Option<&mut QueryComposerGroup> {
        let (index, parent_path) = path.split_last()?;
        match self.nodes_at_path_mut(kind, parent_path)?.get_mut(*index)? {
            QueryComposerNode::Group(group) => Some(group),
            QueryComposerNode::Rule(_) => None,
        }
    }

    #[must_use]
    pub fn validation_code(&self) -> Option<&'static str> {
        if self.all.is_empty() && self.any.is_empty() && self.none.is_empty() {
            return Some(SEARCH_QUERY_REQUIRED_CODE);
        }
        self.all
            .iter()
            .chain(&self.any)
            .chain(&self.none)
            .find_map(QueryComposerNode::validation_code)
    }
}

impl QueryComposerNode {
    fn validation_code(&self) -> Option<&'static str> {
        match self {
            Self::Rule(rule) => rule.validation_code(),
            Self::Group(group) => {
                if group.children.is_empty() {
                    Some(SEARCH_QUERY_GROUP_REQUIRED_CODE)
                } else {
                    group
                        .children
                        .iter()
                        .find_map(QueryComposerNode::validation_code)
                }
            }
        }
    }
}

fn nested_nodes<'a>(
    nodes: &'a [QueryComposerNode],
    path: &[usize],
) -> Option<&'a [QueryComposerNode]> {
    let Some((index, remaining)) = path.split_first() else {
        return Some(nodes);
    };
    let QueryComposerNode::Group(group) = nodes.get(*index)? else {
        return None;
    };
    nested_nodes(&group.children, remaining)
}

fn nested_nodes_mut<'a>(
    nodes: &'a mut Vec<QueryComposerNode>,
    path: &[usize],
) -> Option<&'a mut Vec<QueryComposerNode>> {
    let Some((index, remaining)) = path.split_first() else {
        return Some(nodes);
    };
    let QueryComposerNode::Group(group) = nodes.get_mut(*index)? else {
        return None;
    };
    nested_nodes_mut(&mut group.children, remaining)
}

impl QueryComposerRule {
    #[must_use]
    pub fn value_kind(&self) -> &'static str {
        match self.value {
            QueryRuleValue::Text(_) => "text",
            QueryRuleValue::Wildcard(_) => "wildcard",
            QueryRuleValue::Fuzzy { .. } => "fuzzy",
            QueryRuleValue::Phrase { .. } => "phrase",
            QueryRuleValue::Range { .. } => "range",
        }
    }

    #[must_use]
    pub fn primary_value(&self) -> String {
        match &self.value {
            QueryRuleValue::Text(value)
            | QueryRuleValue::Wildcard(value)
            | QueryRuleValue::Fuzzy { value, .. }
            | QueryRuleValue::Phrase { value, .. } => value.clone(),
            QueryRuleValue::Range { start, .. } => start.clone().unwrap_or_default(),
        }
    }

    #[must_use]
    pub fn secondary_value(&self) -> String {
        match &self.value {
            QueryRuleValue::Range { end, .. } => end.clone().unwrap_or_default(),
            QueryRuleValue::Fuzzy { max_edits, .. } => max_edits.to_string(),
            QueryRuleValue::Phrase { slop, .. } => slop.to_string(),
            QueryRuleValue::Text(_) | QueryRuleValue::Wildcard(_) => String::new(),
        }
    }

    pub fn set_value_kind(&mut self, kind: &str) {
        let primary = self.primary_value();
        self.value = match kind {
            "wildcard" => QueryRuleValue::Wildcard(primary),
            "fuzzy" => QueryRuleValue::Fuzzy {
                value: primary,
                max_edits: 2,
            },
            "phrase" => QueryRuleValue::Phrase {
                value: primary,
                slop: 0,
            },
            "range" => QueryRuleValue::Range {
                start: (!primary.is_empty()).then_some(primary),
                end: None,
                inclusive: true,
            },
            _ => QueryRuleValue::Text(primary),
        };
    }

    pub fn set_primary_value(&mut self, value: String) {
        match &mut self.value {
            QueryRuleValue::Text(current)
            | QueryRuleValue::Wildcard(current)
            | QueryRuleValue::Fuzzy { value: current, .. }
            | QueryRuleValue::Phrase { value: current, .. } => *current = value,
            QueryRuleValue::Range { start, .. } => {
                *start = (!value.trim().is_empty()).then_some(value);
            }
        }
    }

    pub fn set_secondary_value(&mut self, value: String) {
        match &mut self.value {
            QueryRuleValue::Range { end, .. } => {
                *end = (!value.trim().is_empty()).then_some(value);
            }
            QueryRuleValue::Fuzzy { max_edits, .. } => {
                *max_edits = value.parse::<usize>().unwrap_or(2).min(2);
            }
            QueryRuleValue::Phrase { slop, .. } => {
                *slop = value.parse::<usize>().unwrap_or_default();
            }
            QueryRuleValue::Text(_) | QueryRuleValue::Wildcard(_) => {}
        }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        (!self.field.is_empty() && self.field.trim().is_empty())
            || match &self.value {
                QueryRuleValue::Text(value)
                | QueryRuleValue::Wildcard(value)
                | QueryRuleValue::Fuzzy { value, .. }
                | QueryRuleValue::Phrase { value, .. } => value.trim().is_empty(),
                QueryRuleValue::Range { start, end, .. } => start.is_none() && end.is_none(),
            }
    }

    fn validation_code(&self) -> Option<&'static str> {
        if self.is_empty() {
            return Some(SEARCH_QUERY_RULE_REQUIRED_CODE);
        }
        match &self.value {
            QueryRuleValue::Wildcard(value) if !value.contains('*') && !value.contains('?') => {
                Some(SEARCH_QUERY_WILDCARD_REQUIRED_CODE)
            }
            QueryRuleValue::Text(_)
            | QueryRuleValue::Wildcard(_)
            | QueryRuleValue::Fuzzy { .. }
            | QueryRuleValue::Phrase { .. }
            | QueryRuleValue::Range { .. } => None,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct QuickFilterProjection {
    pub base_query: String,
    pub media_kind: String,
    pub genre: String,
    pub year_from: String,
    pub year_to: String,
    pub active_count: usize,
    pub clear_media_kind_query: String,
    pub clear_genre_query: String,
    pub clear_year_query: String,
}

#[cfg(feature = "ssr")]
mod core_projection {
    use ersatzrs_core::search::{SearchExpression, SearchField, SearchTerm, SearchValue};

    use super::{
        QueryComposerGroup, QueryComposerModel, QueryComposerNode, QueryComposerRule,
        QueryGroupKind, QueryRuleValue, QuickFilterProjection, SEARCH_QUERY_REQUIRED_CODE,
        SEARCH_QUERY_RULE_REQUIRED_CODE,
    };

    impl QueryComposerModel {
        #[must_use]
        pub fn from_query(query: &str) -> Self {
            Self::from_expression(SearchExpression::parse_user_query(query))
        }

        pub fn from_validated_query(query: &str) -> Result<Self, &'static str> {
            let expression = SearchExpression::parse_validated_user_query(query).map_err(
                |error| match error {
                    ersatzrs_core::search::SearchQueryValidationError::Empty => {
                        SEARCH_QUERY_REQUIRED_CODE
                    }
                    ersatzrs_core::search::SearchQueryValidationError::MissingRuleValue => {
                        SEARCH_QUERY_RULE_REQUIRED_CODE
                    }
                },
            )?;
            let model = Self::from_expression(expression);
            if let Some(code) = model.validation_code() {
                Err(code)
            } else {
                Ok(model)
            }
        }

        #[must_use]
        pub fn to_query_string(&self) -> String {
            self.to_expression().to_query_string()
        }

        #[must_use]
        pub fn quick_filter_projection(query: &str) -> QuickFilterProjection {
            let model = Self::from_query(query);
            let media_kind_indices = matching_rule_indices(&model.all, |rule| {
                rule.field == "type" && matches!(rule.value, QueryRuleValue::Text(_))
            });
            let genre_indices = matching_rule_indices(&model.all, |rule| {
                rule.field == "genre"
                    && matches!(
                        rule.value,
                        QueryRuleValue::Text(_) | QueryRuleValue::Phrase { .. }
                    )
            });
            let year_indices = matching_rule_indices(&model.all, |rule| {
                rule.field == "year" && matches!(rule.value, QueryRuleValue::Range { .. })
            });

            let media_kind = one_rule(&model.all, &media_kind_indices)
                .map(QueryComposerRule::primary_value)
                .unwrap_or_default();
            let genre = one_rule(&model.all, &genre_indices)
                .map(QueryComposerRule::primary_value)
                .unwrap_or_default();
            let (year_from, year_to) = one_rule(&model.all, &year_indices).map_or_else(
                || (String::new(), String::new()),
                |rule| (rule.primary_value(), rule.secondary_value()),
            );

            let mut base = model.clone();
            remove_rules(
                &mut base.all,
                [&media_kind_indices, &genre_indices, &year_indices],
            );

            let clear_media_kind_query = query_without_one_rule(&model, &media_kind_indices);
            let clear_genre_query = query_without_one_rule(&model, &genre_indices);
            let clear_year_query = query_without_one_rule(&model, &year_indices);

            QuickFilterProjection {
                base_query: base.to_query_string(),
                active_count: usize::from(!media_kind.is_empty())
                    + usize::from(!genre.is_empty())
                    + usize::from(!year_from.is_empty() || !year_to.is_empty()),
                media_kind,
                genre,
                year_from,
                year_to,
                clear_media_kind_query,
                clear_genre_query,
                clear_year_query,
            }
        }

        fn from_expression(expression: SearchExpression) -> Self {
            let mut model = Self::default();
            match expression {
                SearchExpression::MatchAll => {}
                SearchExpression::And(children) => {
                    let mut promoted_any = false;
                    for child in children {
                        match child {
                            SearchExpression::Or(children) if !promoted_any => {
                                model
                                    .any
                                    .extend(children.into_iter().map(node_from_expression));
                                promoted_any = true;
                            }
                            SearchExpression::Not(child) => {
                                append_none_expression(&mut model.none, *child);
                            }
                            other => model.all.push(node_from_expression(other)),
                        }
                    }
                }
                SearchExpression::Or(children) => {
                    model
                        .any
                        .extend(children.into_iter().map(node_from_expression));
                }
                SearchExpression::Not(child) => append_none_expression(&mut model.none, *child),
                term @ SearchExpression::Term(_) => model.all.push(node_from_expression(term)),
            }
            model
        }

        fn to_expression(&self) -> SearchExpression {
            let mut children = self
                .all
                .iter()
                .cloned()
                .map(expression_from_node)
                .collect::<Vec<_>>();
            if !self.any.is_empty() {
                children.push(combine_expression(
                    QueryGroupKind::Any,
                    self.any.iter().cloned().map(expression_from_node).collect(),
                ));
            }
            if !self.none.is_empty() {
                children.push(combine_expression(
                    QueryGroupKind::None,
                    self.none
                        .iter()
                        .cloned()
                        .map(expression_from_node)
                        .collect(),
                ));
            }
            combine_expression(QueryGroupKind::All, children)
        }
    }

    impl QueryComposerRule {
        fn from_term(term: SearchTerm) -> Self {
            Self {
                field: term.field.query_token().to_owned(),
                value: match term.value {
                    SearchValue::Text(value) => QueryRuleValue::Text(value),
                    SearchValue::Wildcard(value) => QueryRuleValue::Wildcard(value),
                    SearchValue::Fuzzy { value, max_edits } => {
                        QueryRuleValue::Fuzzy { value, max_edits }
                    }
                    SearchValue::Phrase { value, slop } => QueryRuleValue::Phrase { value, slop },
                    SearchValue::Range {
                        start,
                        end,
                        inclusive,
                    } => QueryRuleValue::Range {
                        start,
                        end,
                        inclusive,
                    },
                },
            }
        }

        fn into_term(self) -> SearchTerm {
            SearchTerm {
                field: SearchField::from_query_token(&self.field),
                value: match self.value {
                    QueryRuleValue::Text(value) => SearchValue::Text(value),
                    QueryRuleValue::Wildcard(value) => SearchValue::Wildcard(value),
                    QueryRuleValue::Fuzzy { value, max_edits } => {
                        SearchValue::Fuzzy { value, max_edits }
                    }
                    QueryRuleValue::Phrase { value, slop } => SearchValue::Phrase { value, slop },
                    QueryRuleValue::Range {
                        start,
                        end,
                        inclusive,
                    } => SearchValue::Range {
                        start,
                        end,
                        inclusive,
                    },
                },
            }
        }
    }

    fn append_none_expression(target: &mut Vec<QueryComposerNode>, expression: SearchExpression) {
        match expression {
            SearchExpression::Or(children) => {
                target.extend(children.into_iter().map(node_from_expression));
            }
            other => target.push(node_from_expression(other)),
        }
    }

    fn node_from_expression(expression: SearchExpression) -> QueryComposerNode {
        match expression {
            SearchExpression::Term(term) => {
                QueryComposerNode::Rule(QueryComposerRule::from_term(term))
            }
            SearchExpression::MatchAll => QueryComposerNode::Group(QueryComposerGroup {
                kind: QueryGroupKind::All,
                children: Vec::new(),
            }),
            SearchExpression::And(children) => QueryComposerNode::Group(QueryComposerGroup {
                kind: QueryGroupKind::All,
                children: children.into_iter().map(node_from_expression).collect(),
            }),
            SearchExpression::Or(children) => QueryComposerNode::Group(QueryComposerGroup {
                kind: QueryGroupKind::Any,
                children: children.into_iter().map(node_from_expression).collect(),
            }),
            SearchExpression::Not(child) => {
                let children = match *child {
                    SearchExpression::Or(children) => {
                        children.into_iter().map(node_from_expression).collect()
                    }
                    other => vec![node_from_expression(other)],
                };
                QueryComposerNode::Group(QueryComposerGroup {
                    kind: QueryGroupKind::None,
                    children,
                })
            }
        }
    }

    fn expression_from_node(node: QueryComposerNode) -> SearchExpression {
        match node {
            QueryComposerNode::Rule(rule) => SearchExpression::Term(rule.into_term()),
            QueryComposerNode::Group(group) => combine_expression(
                group.kind,
                group
                    .children
                    .into_iter()
                    .map(expression_from_node)
                    .collect(),
            ),
        }
    }

    fn combine_expression(
        kind: QueryGroupKind,
        mut children: Vec<SearchExpression>,
    ) -> SearchExpression {
        children.retain(|child| !matches!(child, SearchExpression::MatchAll));
        let one_or_group = |mut children: Vec<SearchExpression>, or: bool| match children.len() {
            0 => SearchExpression::MatchAll,
            1 => children.remove(0),
            _ if or => SearchExpression::Or(children),
            _ => SearchExpression::And(children),
        };
        match kind {
            QueryGroupKind::All => one_or_group(children, false),
            QueryGroupKind::Any => one_or_group(children, true),
            QueryGroupKind::None => SearchExpression::Not(Box::new(one_or_group(children, true))),
        }
    }

    fn matching_rule_indices(
        nodes: &[QueryComposerNode],
        predicate: impl Fn(&QueryComposerRule) -> bool,
    ) -> Vec<usize> {
        nodes
            .iter()
            .enumerate()
            .filter_map(|(index, node)| match node {
                QueryComposerNode::Rule(rule) if predicate(rule) => Some(index),
                QueryComposerNode::Rule(_) | QueryComposerNode::Group(_) => None,
            })
            .collect()
    }

    fn one_rule<'a>(
        nodes: &'a [QueryComposerNode],
        indices: &[usize],
    ) -> Option<&'a QueryComposerRule> {
        let [index] = indices else {
            return None;
        };
        match nodes.get(*index) {
            Some(QueryComposerNode::Rule(rule)) => Some(rule),
            Some(QueryComposerNode::Group(_)) | None => None,
        }
    }

    fn remove_one_rule(nodes: &mut Vec<QueryComposerNode>, indices: &[usize]) {
        if let [index] = indices {
            nodes.remove(*index);
        }
    }

    fn remove_rules<const N: usize>(nodes: &mut Vec<QueryComposerNode>, index_sets: [&[usize]; N]) {
        let mut indices = index_sets
            .into_iter()
            .filter_map(|indices| match indices {
                [index] => Some(*index),
                _ => None,
            })
            .collect::<Vec<_>>();
        indices.sort_unstable_by(|left, right| right.cmp(left));
        for index in indices {
            nodes.remove(index);
        }
    }

    fn query_without_one_rule(model: &QueryComposerModel, indices: &[usize]) -> String {
        let mut without = model.clone();
        remove_one_rule(&mut without.all, indices);
        without.to_query_string()
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn shallow_query_projects_to_all_any_and_none_without_losing_meaning() {
            let query = "type:movie AND year:[1980 TO 1999] AND (genre:\"Science Fiction\" OR genre:\"Space Opera\") AND NOT (content_rating:NC-17 OR has_commentary:true)";
            let model = QueryComposerModel::from_query(query);

            assert_eq!(model.all.len(), 2);
            assert_eq!(model.any.len(), 2);
            assert_eq!(model.none.len(), 2);
            assert_eq!(
                model.to_query_string(),
                "type:movie AND year:[1980 TO 1999] AND (genre:\"Science Fiction\" OR genre:\"Space Opera\") AND NOT (content_rating:NC-17 OR has_commentary:true)"
            );
        }

        #[test]
        fn second_or_group_remains_nested_in_match_all() {
            let model = QueryComposerModel::from_query(
                "(genre:drama OR genre:comedy) AND (actor:Alice OR actor:Bob)",
            );

            assert_eq!(model.any.len(), 2);
            assert!(matches!(
                model.all.as_slice(),
                [QueryComposerNode::Group(_)]
            ));
            assert_eq!(
                model.to_query_string(),
                "(actor:Alice OR actor:Bob) AND (genre:drama OR genre:comedy)"
            );
        }

        #[test]
        fn validated_projection_rejects_incomplete_rules() {
            assert_eq!(
                QueryComposerModel::from_validated_query("title:"),
                Err(SEARCH_QUERY_RULE_REQUIRED_CODE)
            );
            assert_eq!(
                QueryComposerModel::from_validated_query("( )"),
                Err(SEARCH_QUERY_RULE_REQUIRED_CODE)
            );
        }

        #[test]
        fn composer_round_trip_preserves_multiword_value_kinds() {
            let model = QueryComposerModel {
                all: vec![
                    QueryComposerNode::Rule(QueryComposerRule {
                        field: "title".to_owned(),
                        value: QueryRuleValue::Text("Star Wars".to_owned()),
                    }),
                    QueryComposerNode::Rule(QueryComposerRule {
                        field: "genre".to_owned(),
                        value: QueryRuleValue::Wildcard("Science Fic*".to_owned()),
                    }),
                ],
                ..QueryComposerModel::default()
            };

            let query = model.to_query_string();
            assert_eq!(query, "title:Star\\ Wars AND genre:Science\\ Fic*");
            assert_eq!(QueryComposerModel::from_validated_query(&query), Ok(model));
        }

        #[test]
        fn wildcard_rule_requires_an_actual_wildcard_marker() {
            let model = QueryComposerModel {
                all: vec![QueryComposerNode::Rule(QueryComposerRule {
                    field: "genre".to_owned(),
                    value: QueryRuleValue::Wildcard("drama".to_owned()),
                })],
                ..QueryComposerModel::default()
            };

            assert_eq!(
                model.validation_code(),
                Some(super::super::SEARCH_QUERY_WILDCARD_REQUIRED_CODE)
            );
        }

        #[test]
        fn quick_filters_extract_only_unambiguous_top_level_rules() {
            let projection = QueryComposerModel::quick_filter_projection(
                "space AND type:movie AND genre:\"Science Fiction\" AND year:[1980 TO 1999]",
            );

            assert_eq!(projection.base_query, "space");
            assert_eq!(projection.media_kind, "movie");
            assert_eq!(projection.genre, "Science Fiction");
            assert_eq!(projection.year_from, "1980");
            assert_eq!(projection.year_to, "1999");
            assert_eq!(projection.active_count, 3);
            assert_eq!(
                projection.clear_genre_query,
                "space AND type:movie AND year:[1980 TO 1999]"
            );
        }

        #[test]
        fn nested_path_helpers_edit_only_the_focused_child_group() {
            let mut model = QueryComposerModel::from_query(
                "type:movie AND ((genre:drama OR genre:comedy) AND actor:Alice)",
            );
            let group_index = model
                .all
                .iter()
                .position(QueryComposerNode::is_nested)
                .expect("nested group");

            let group = model
                .group_at_path_mut(QueryGroupKind::All, &[group_index])
                .expect("focused group");
            group.kind = QueryGroupKind::Any;

            assert!(model
                .nodes_at_path(QueryGroupKind::All, &[group_index])
                .is_some_and(|children| !children.is_empty()));
            assert!(model.to_query_string().contains("type:movie"));
        }
    }
}
