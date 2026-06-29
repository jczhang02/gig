use axum::http::HeaderMap;

pub fn token_is_valid(headers: &HeaderMap, expected: &str) -> bool {
    headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(parse_bearer_token)
        .is_some_and(|token| constant_time_eq(token.as_bytes(), expected.as_bytes()))
}

fn parse_bearer_token(value: &str) -> Option<&str> {
    let mut parts = value.split_ascii_whitespace();
    let scheme = parts.next()?;
    let token = parts.next()?;
    if parts.next().is_some() || !scheme.eq_ignore_ascii_case("bearer") {
        return None;
    }
    Some(token)
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }

    let mut diff = 0u8;
    for (left, right) in a.iter().zip(b.iter()) {
        diff |= left ^ right;
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::header::AUTHORIZATION;

    #[test]
    fn validates_bearer_token() {
        let mut headers = HeaderMap::new();
        headers.insert(AUTHORIZATION, "Bearer secret-token".parse().unwrap());

        assert!(token_is_valid(&headers, "secret-token"));
        assert!(!token_is_valid(&headers, "other-token"));

        headers.insert(AUTHORIZATION, "bearer secret-token".parse().unwrap());
        assert!(token_is_valid(&headers, "secret-token"));
    }

    #[test]
    fn rejects_missing_or_malformed_auth() {
        let mut headers = HeaderMap::new();
        assert!(!token_is_valid(&headers, "secret-token"));

        headers.insert(AUTHORIZATION, "Basic secret-token".parse().unwrap());
        assert!(!token_is_valid(&headers, "secret-token"));

        headers.insert(AUTHORIZATION, "Bearer secret-token extra".parse().unwrap());
        assert!(!token_is_valid(&headers, "secret-token"));
    }
}
