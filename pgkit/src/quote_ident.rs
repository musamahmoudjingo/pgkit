/// Quotes a SQL identifier at compile time, so mixed-case and reserved-word
/// names survive Postgres, which lowercases every unquoted identifier.
///
/// One argument quotes a single identifier; two quote a qualified
/// `"qualifier"."name"` pair (never `quote_ident!("l.order_id")` — the dot
/// would become part of one quoted name).
///
/// The name is pasted verbatim between the quotes: it must be a string
/// literal and must not itself contain a `"`.
///
/// # Example
/// ```
/// use pgkit::quote_ident;
///
/// assert_eq!(quote_ident!("userName"), r#""userName""#);
/// assert_eq!(quote_ident!("order"), r#""order""#);
/// assert_eq!(quote_ident!("u", "userName"), r#""u"."userName""#);
/// ```
///
/// In a query:
/// ```
/// # use pgkit::query_builder::WhereBuilder;
/// # use pgkit::query_builder::operators::*;
/// use pgkit::quote_ident;
///
/// # let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new("");
/// WhereBuilder::new()
///     .where_col(quote_ident!("userName"), Equal, "musa")
///     .apply_to(&mut query);
/// assert_eq!(query.sql(), r#" WHERE "userName" = $1"#);
/// ```
#[macro_export]
macro_rules! quote_ident {
    ($name:literal) => {
        concat!('"', $name, '"')
    };
    ($qualifier:literal, $name:literal) => {
        concat!('"', $qualifier, '"', '.', '"', $name, '"')
    };
}
