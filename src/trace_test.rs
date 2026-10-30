#[test]
fn trace() {
    for p in [r"\p{IsBasicLatin}+", r"\P{IsLatin-1Supplement}", r"\p{IsGreek}", r"\p{L}"] {
        println!("TRACE {p} => {:?}", moxml::xsd_regex::translate(p).map(|r| r.as_str().to_string()));
    }
}
