//! Module doc comment.
// A line comment.

/// Doc comment on main.
fn main() {
    /* nested /* block */ comment */
    let url = "https://example.com/#anchor"; // trailing comment
    let s = "not // a comment";
    /* multi
       line
       block */
    println!("{url} {s}");
}
