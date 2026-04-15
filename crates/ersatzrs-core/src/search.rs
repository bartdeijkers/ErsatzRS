use chrono::{DateTime, Datelike, Utc};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchExpression {
    MatchAll,
    Term(SearchTerm),
    And(Vec<SearchExpression>),
    Or(Vec<SearchExpression>),
    Not(Box<SearchExpression>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchTerm {
    pub field: SearchField,
    pub value: SearchValue,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchField {
    Default,
    Title,
    SortTitle,
    OriginalTitle,
    Year,
    ReleaseDate,
    Genre,
    Tag,
    Studio,
    Actor,
    Director,
    Writer,
    MediaKind,
    ShowTitle,
    ShowGenre,
    ShowTag,
    ShowStudio,
    SeasonNumber,
    EpisodeNumber,
    MetadataKind,
    ProviderGuid(Option<String>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchValue {
    Text(String),
    Wildcard(String),
    Range {
        start: Option<String>,
        end: Option<String>,
        inclusive: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SearchDocument {
    pub media_item_id: i64,
    pub media_kind: String,
    pub metadata_kind: String,
    pub title: Option<String>,
    pub sort_title: Option<String>,
    pub original_title: Option<String>,
    pub year: Option<i32>,
    pub release_date: Option<DateTime<Utc>>,
    pub genres: Vec<String>,
    pub tags: Vec<String>,
    pub studios: Vec<String>,
    pub actors: Vec<String>,
    pub directors: Vec<String>,
    pub writers: Vec<String>,
    pub show_title: Option<String>,
    pub show_genres: Vec<String>,
    pub show_tags: Vec<String>,
    pub show_studios: Vec<String>,
    pub season_number: Option<i32>,
    pub episode_number: Option<i32>,
    pub provider_guids: Vec<SearchProviderGuid>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchProviderGuid {
    pub source: String,
    pub value: String,
}

impl SearchExpression {
    #[must_use]
    pub fn parse(query: &str) -> Self {
        SearchParser::new(query).parse()
    }

    #[must_use]
    pub fn matches(&self, document: &SearchDocument) -> bool {
        match self {
            Self::MatchAll => true,
            Self::Term(term) => term.matches(document),
            Self::And(expressions) => expressions
                .iter()
                .all(|expression| expression.matches(document)),
            Self::Or(expressions) => expressions
                .iter()
                .any(|expression| expression.matches(document)),
            Self::Not(expression) => !expression.matches(document),
        }
    }
}

impl SearchTerm {
    #[must_use]
    pub fn matches(&self, document: &SearchDocument) -> bool {
        let values = self.field.values(document);
        match &self.value {
            SearchValue::Text(expected) => values
                .iter()
                .any(|actual| text_matches(actual, expected, false)),
            SearchValue::Wildcard(pattern) => values
                .iter()
                .any(|actual| text_matches(actual, pattern, true)),
            SearchValue::Range {
                start,
                end,
                inclusive,
            } => values
                .iter()
                .any(|actual| range_matches(actual, start.as_deref(), end.as_deref(), *inclusive)),
        }
    }
}

impl SearchField {
    fn values(&self, document: &SearchDocument) -> Vec<String> {
        match self {
            Self::Default => option_values([
                document.title.as_deref(),
                document.sort_title.as_deref(),
                document.original_title.as_deref(),
                document.show_title.as_deref(),
            ]),
            Self::Title => option_values([document.title.as_deref()]),
            Self::SortTitle => option_values([document.sort_title.as_deref()]),
            Self::OriginalTitle => option_values([document.original_title.as_deref()]),
            Self::Year => document
                .year
                .map(|year| year.to_string())
                .into_iter()
                .collect(),
            Self::ReleaseDate => document
                .release_date
                .map(|date| format!("{:04}{:02}{:02}", date.year(), date.month(), date.day()))
                .into_iter()
                .collect(),
            Self::Genre => document.genres.clone(),
            Self::Tag => document.tags.clone(),
            Self::Studio => document.studios.clone(),
            Self::Actor => document.actors.clone(),
            Self::Director => document.directors.clone(),
            Self::Writer => document.writers.clone(),
            Self::MediaKind => vec![document.media_kind.clone()],
            Self::ShowTitle => option_values([document.show_title.as_deref()]),
            Self::ShowGenre => document.show_genres.clone(),
            Self::ShowTag => document.show_tags.clone(),
            Self::ShowStudio => document.show_studios.clone(),
            Self::SeasonNumber => document
                .season_number
                .map(|number| number.to_string())
                .into_iter()
                .collect(),
            Self::EpisodeNumber => document
                .episode_number
                .map(|number| number.to_string())
                .into_iter()
                .collect(),
            Self::MetadataKind => vec![document.metadata_kind.clone()],
            Self::ProviderGuid(None) => document
                .provider_guids
                .iter()
                .flat_map(|guid| {
                    [
                        guid.value.clone(),
                        format!("{}://{}", guid.source, guid.value),
                    ]
                })
                .collect(),
            Self::ProviderGuid(Some(source)) => document
                .provider_guids
                .iter()
                .filter(|guid| guid.source.eq_ignore_ascii_case(source))
                .map(|guid| guid.value.clone())
                .collect(),
        }
    }

    fn parse(field: &str) -> Self {
        match field.to_ascii_lowercase().as_str() {
            "title" => Self::Title,
            "sort_title" | "sorttitle" => Self::SortTitle,
            "original_title" | "originaltitle" => Self::OriginalTitle,
            "year" => Self::Year,
            "release_date" | "released" | "released_on" => Self::ReleaseDate,
            "genre" => Self::Genre,
            "tag" | "tag_full" => Self::Tag,
            "studio" => Self::Studio,
            "actor" => Self::Actor,
            "director" => Self::Director,
            "writer" => Self::Writer,
            "type" | "kind" | "media_kind" => Self::MediaKind,
            "show_title" => Self::ShowTitle,
            "show_genre" => Self::ShowGenre,
            "show_tag" => Self::ShowTag,
            "show_studio" => Self::ShowStudio,
            "season_number" | "season" => Self::SeasonNumber,
            "episode_number" | "episode" => Self::EpisodeNumber,
            "metadata_kind" => Self::MetadataKind,
            "guid" | "provider_guid" => Self::ProviderGuid(None),
            source => Self::ProviderGuid(Some(source.to_owned())),
        }
    }
}

fn option_values<const N: usize>(values: [Option<&str>; N]) -> Vec<String> {
    values
        .into_iter()
        .flatten()
        .map(ToOwned::to_owned)
        .collect()
}

fn text_matches(actual: &str, expected: &str, wildcard: bool) -> bool {
    let actual = actual.to_ascii_lowercase();
    let expected = expected.to_ascii_lowercase();
    if wildcard || expected.contains('*') || expected.contains('?') {
        wildcard_matches(&actual, &expected)
    } else {
        actual.contains(&expected)
    }
}

fn range_matches(actual: &str, start: Option<&str>, end: Option<&str>, inclusive: bool) -> bool {
    let actual = normalize_range_value(actual);
    let start_matches = start.map(normalize_range_value).is_none_or(|start| {
        if inclusive {
            actual >= start
        } else {
            actual > start
        }
    });
    let end_matches = end.map(normalize_range_value).is_none_or(|end| {
        if inclusive {
            actual <= end
        } else {
            actual < end
        }
    });
    start_matches && end_matches
}

fn normalize_range_value(value: &str) -> String {
    value
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .collect::<String>()
        .to_ascii_lowercase()
}

fn wildcard_matches(actual: &str, pattern: &str) -> bool {
    let actual = actual.as_bytes();
    let pattern = pattern.as_bytes();
    let (mut ai, mut pi) = (0, 0);
    let mut star = None;
    let mut match_after_star = 0;

    while ai < actual.len() {
        if pi < pattern.len() && (pattern[pi] == b'?' || pattern[pi] == actual[ai]) {
            ai += 1;
            pi += 1;
        } else if pi < pattern.len() && pattern[pi] == b'*' {
            star = Some(pi);
            match_after_star = ai;
            pi += 1;
        } else if let Some(star_index) = star {
            pi = star_index + 1;
            match_after_star += 1;
            ai = match_after_star;
        } else {
            return false;
        }
    }

    while pi < pattern.len() && pattern[pi] == b'*' {
        pi += 1;
    }

    pi == pattern.len()
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
    Word(String),
    Phrase(String),
    Colon,
    LeftParen,
    RightParen,
    LeftBracket,
    RightBracket,
    LeftBrace,
    RightBrace,
    To,
    And,
    Or,
    Not,
    Plus,
    Minus,
}

struct SearchParser {
    tokens: Vec<Token>,
    position: usize,
}

impl SearchParser {
    fn new(query: &str) -> Self {
        Self {
            tokens: tokenize(query),
            position: 0,
        }
    }

    fn parse(mut self) -> SearchExpression {
        if self.tokens.is_empty() {
            return SearchExpression::MatchAll;
        }

        self.parse_or()
    }

    fn parse_or(&mut self) -> SearchExpression {
        let mut expressions = vec![self.parse_and()];
        while self.consume(&Token::Or) {
            expressions.push(self.parse_and());
        }

        if expressions.len() == 1 {
            expressions.remove(0)
        } else {
            SearchExpression::Or(expressions)
        }
    }

    fn parse_and(&mut self) -> SearchExpression {
        let mut expressions = vec![self.parse_unary()];
        while !self.is_done() && !self.check(&Token::RightParen) && !self.check(&Token::Or) {
            let _ = self.consume(&Token::And);
            expressions.push(self.parse_unary());
        }

        if expressions.len() == 1 {
            expressions.remove(0)
        } else {
            SearchExpression::And(expressions)
        }
    }

    fn parse_unary(&mut self) -> SearchExpression {
        if self.consume(&Token::Not) || self.consume(&Token::Minus) {
            SearchExpression::Not(Box::new(self.parse_unary()))
        } else {
            let _ = self.consume(&Token::Plus);
            self.parse_primary()
        }
    }

    fn parse_primary(&mut self) -> SearchExpression {
        if self.consume(&Token::LeftParen) {
            let expression = self.parse_or();
            let _ = self.consume(&Token::RightParen);
            return expression;
        }

        let Some(token) = self.advance() else {
            return SearchExpression::MatchAll;
        };

        let (field, value_token) = if let Token::Word(field_name) = &token {
            if self.consume(&Token::Colon)
                || self.consume_word("=")
                || self.consume_word("==")
                || self.consume_word("contains")
            {
                (SearchField::parse(field_name), self.advance())
            } else {
                (SearchField::Default, Some(token))
            }
        } else {
            (SearchField::Default, Some(token))
        };

        let value = match value_token {
            Some(Token::LeftBracket) => self.parse_range(true),
            Some(Token::LeftBrace) => self.parse_range(false),
            Some(Token::Phrase(value) | Token::Word(value)) if value.contains(['*', '?']) => {
                SearchValue::Wildcard(value)
            }
            Some(Token::Phrase(value) | Token::Word(value)) => SearchValue::Text(value),
            Some(other) => SearchValue::Text(token_text(&other)),
            None => SearchValue::Text(String::new()),
        };

        SearchExpression::Term(SearchTerm { field, value })
    }

    fn parse_range(&mut self, inclusive: bool) -> SearchValue {
        let start = self.parse_range_part();
        let _ = self.consume(&Token::To);
        let end = self.parse_range_part();
        if inclusive {
            let _ = self.consume(&Token::RightBracket);
        } else {
            let _ = self.consume(&Token::RightBrace);
        }

        SearchValue::Range {
            start: start.filter(|value| value != "*"),
            end: end.filter(|value| value != "*"),
            inclusive,
        }
    }

    fn parse_range_part(&mut self) -> Option<String> {
        match self.advance() {
            Some(Token::Word(value) | Token::Phrase(value)) => Some(value),
            Some(token) => Some(token_text(&token)),
            None => None,
        }
    }

    fn advance(&mut self) -> Option<Token> {
        let token = self.tokens.get(self.position).cloned();
        if token.is_some() {
            self.position += 1;
        }
        token
    }

    fn consume(&mut self, expected: &Token) -> bool {
        if self.check(expected) {
            self.position += 1;
            true
        } else {
            false
        }
    }

    fn consume_word(&mut self, expected: &str) -> bool {
        if self.tokens.get(self.position).is_some_and(
            |token| matches!(token, Token::Word(value) if value.eq_ignore_ascii_case(expected)),
        ) {
            self.position += 1;
            true
        } else {
            false
        }
    }

    fn check(&self, expected: &Token) -> bool {
        self.tokens
            .get(self.position)
            .is_some_and(|token| token == expected)
    }

    fn is_done(&self) -> bool {
        self.position >= self.tokens.len()
    }
}

fn tokenize(query: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut chars = query.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            ch if ch.is_whitespace() => {}
            ':' => tokens.push(Token::Colon),
            '(' => tokens.push(Token::LeftParen),
            ')' => tokens.push(Token::RightParen),
            '[' => tokens.push(Token::LeftBracket),
            ']' => tokens.push(Token::RightBracket),
            '{' => tokens.push(Token::LeftBrace),
            '}' => tokens.push(Token::RightBrace),
            '+' => tokens.push(Token::Plus),
            '-' => tokens.push(Token::Minus),
            '"' | '\'' => {
                let quote = ch;
                let mut value = String::new();
                for next in chars.by_ref() {
                    if next == quote {
                        break;
                    }
                    value.push(next);
                }
                tokens.push(Token::Phrase(value));
            }
            _ => {
                let mut value = String::from(ch);
                while let Some(next) = chars.peek().copied() {
                    if next.is_whitespace()
                        || matches!(next, ':' | '(' | ')' | '[' | ']' | '{' | '}')
                    {
                        break;
                    }
                    value.push(next);
                    let _ = chars.next();
                }
                tokens.push(word_token(value));
            }
        }
    }

    tokens
}

fn word_token(value: String) -> Token {
    match value.to_ascii_uppercase().as_str() {
        "AND" => Token::And,
        "OR" => Token::Or,
        "NOT" => Token::Not,
        "TO" => Token::To,
        _ => Token::Word(value),
    }
}

fn token_text(token: &Token) -> String {
    match token {
        Token::Word(value) | Token::Phrase(value) => value.clone(),
        Token::Colon => ":".to_owned(),
        Token::LeftParen => "(".to_owned(),
        Token::RightParen => ")".to_owned(),
        Token::LeftBracket => "[".to_owned(),
        Token::RightBracket => "]".to_owned(),
        Token::LeftBrace => "{".to_owned(),
        Token::RightBrace => "}".to_owned(),
        Token::To => "TO".to_owned(),
        Token::And => "AND".to_owned(),
        Token::Or => "OR".to_owned(),
        Token::Not => "NOT".to_owned(),
        Token::Plus => "+".to_owned(),
        Token::Minus => "-".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn document() -> SearchDocument {
        SearchDocument {
            media_item_id: 1,
            media_kind: "movie".to_owned(),
            metadata_kind: "Movie".to_owned(),
            title: Some("The Great Escape".to_owned()),
            sort_title: Some("great escape".to_owned()),
            original_title: Some("Original Escape".to_owned()),
            year: Some(1963),
            release_date: DateTime::parse_from_rfc3339("1963-07-04T00:00:00Z")
                .ok()
                .map(|date| date.with_timezone(&Utc)),
            genres: vec!["Adventure".to_owned()],
            tags: vec!["Classic".to_owned()],
            studios: vec!["United Artists".to_owned()],
            actors: vec!["Steve McQueen".to_owned()],
            show_title: Some("Not a Show".to_owned()),
            provider_guids: vec![SearchProviderGuid {
                source: "tmdb".to_owned(),
                value: "5925".to_owned(),
            }],
            ..SearchDocument::default()
        }
    }

    #[test]
    fn parses_fielded_boolean_queries() {
        let query =
            SearchExpression::parse("genre:Adventure AND actor:\"Steve McQueen\" -tag:Noir");
        assert!(query.matches(&document()));
    }

    #[test]
    fn parses_ranges_and_provider_guid_fields() {
        let doc = document();
        assert!(SearchExpression::parse("year:[1960 TO 1970] AND tmdb:5925").matches(&doc));
        assert!(SearchExpression::parse("release_date:[19630701 TO 19630731]").matches(&doc));
        assert!(!SearchExpression::parse("type:episode OR guid:missing").matches(&doc));
    }
}
