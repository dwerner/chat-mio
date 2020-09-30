use nom::{alt, do_parse, eat_separator, named, tag, take_until};

use std::error::Error;

named!(pub space<&str, &str>, eat_separator!(&" \t"[..]));

named!(get<&str, http::Method>,
    do_parse!(
        tag!("GET") >>
        ( http::Method::GET )
    )
);

named!(post<&str, http::Method>,
    do_parse!(
        tag!("POST") >>
        ( http::Method::POST )
    )
);

named!(method<&str, http::Method>, alt!(get|post));

named!(uri<&str, &str>,
    do_parse!(
        path: take_until!("HTTP") >>
        ( path.trim() )
    )
);

named!(http11<&str, http::Version>,
    do_parse!(
        tag!("HTTP/1.1") >>
        take_until!("\r\n") >>
        tag!("\r\n") >>
        ( http::Version::HTTP_11 )
    )
);

named!(start_line<&str, (http::Method, &str, http::Version)>,
    do_parse!(
        method : method >>
        uri : uri >>
        version: http11 >>
        (method, uri, version)
    )
);

named!(pub header<&str, (&str, &str)>,
    do_parse!(
        header_name: take_until!(":") >>
        tag!(":") >>
        header_value: take_until!("\r\n") >>
        tag!("\r\n") >>
        (
            header_name.trim(),
            header_value.trim()
        )
    )
);

named!(pub end_headers<&str, &str>,
    tag!("\r\n")
);

///
/// Parse a buffer of (potentially) multiple pipelined http requests
///
pub fn parse_buffer(buffer: &[u8]) -> Result<Vec<http::Request<&str>>, Box<dyn Error>> {
    let buffer_str = std::str::from_utf8(buffer)?;
    let mut requests = Vec::new();
    let mut temp_buffer = buffer_str;
    while !temp_buffer.is_empty() {
        let (remaining, request) = match parse_http_request(temp_buffer) {
            Ok(item) => item,
            Err(e) => {
                eprintln!(
                    "error parsing: {:?}\ntemp_buffer:\n[[[{}]]]",
                    e, temp_buffer
                );
                return Err(e);
            }
        };
        requests.push(request);
        temp_buffer = remaining;
    }
    Ok(requests)
}

///
/// Parse a single http request from a buffer, returning the remainder of the buffer once
/// Content-Length is reached
///
pub fn parse_http_request(buffer: &str) -> Result<(&str, http::Request<&str>), Box<dyn Error>> {
    let mut builder = http::Request::builder();
    let mut temp_buffer = buffer;
    temp_buffer = match start_line(temp_buffer) {
        Ok((remainder, (method, uri, version))) => {
            builder.method(method).uri(uri).version(version);
            remainder
        }
        Err(e) => return Err(format!("unable to parse http start line {:?}", e).into()),
    };

    let mut len = 0;
    while let Ok((remainder, (header, value))) = header(temp_buffer) {
        if header.trim() == "Content-Length" {
            if let Ok(l) = value.parse::<usize>() {
                len = l;
            }
        }
        builder.header(header, value);
        temp_buffer = remainder;
        if let Ok((remainder, _)) = end_headers(remainder) {
            temp_buffer = remainder;
            break;
        }
    }
    if temp_buffer.len() >= len {
        Ok((&temp_buffer[len..], builder.body(&temp_buffer[..len])?))
    } else {
        Err("incomplete request".into())
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn test_single_request() {
        let requests =
            parse_buffer(
            b"GET /something/neat/here/1 HTTP/1.1\r\nUser-Agent: Wget/1.20.1 (linux-gnu)\r\nAccept: */*\r\n Accept-Encoding: identity\r\n Host: localhost:8080\r\nConnection: Keep-Alive\r\nContent-Length: 16\r\n\r\n{'kinda':'json'}").unwrap();

        assert_eq!(requests.len(), 1);
    }

    #[test]
    fn test_pipelined_requests_with_body() {
        let requests =
            parse_buffer(
            b"GET /something/neat/here/1 HTTP/1.1\r\nUser-Agent: Wget/1.20.1 (linux-gnu)\r\nAccept: */*\r\n Accept-Encoding: identity\r\n Host: localhost:8080\r\nConnection: Keep-Alive\r\nContent-Length: 15\r\n\r\n{'an':'object'}GET /something/neat/here/1 HTTP/1.1\r\nUser-Agent: Wget/1.20.1 (linux-gnu)\r\nAccept: */*\r\n Accept-Encoding: identity\r\n Host: localhost:8080\r\nConnection: Keep-Alive\r\nContent-Length: 15\r\n\r\n{'an':'object'}").unwrap();

        assert_eq!(requests.len(), 2);
    }

    #[test]
    fn test_parse_pipelined_no_body() {
        let req =
            parse_buffer(b"GET /something/here HTTP/1.1\r\nUser-Agent: something\r\n\r\nGET /something/here HTTP/1.1\r\nUser-Agent: something\r\n\r\n")
                .unwrap();
    }

    #[test]
    fn test_bad_content_length() {
        let r =
            parse_http_request(
            "GET /something/neat/here/1 HTTP/1.1\r\nUser-Agent: Wget/1.20.1 (linux-gnu)\r\nAccept: */*\r\n Accept-Encoding: identity\r\n Host: localhost:8080\r\nConnection: Keep-Alive\r\nContent-Length: 9000\r\n\r\nthis was a body...");
        assert!(r.is_err());
    }

    #[test]
    fn test_parse_real() {
        let (_, req) =
            parse_http_request(
            "GET /something/neat/here/1 HTTP/1.1\r\nUser-Agent: Wget/1.20.1 (linux-gnu)\r\nAccept: */*\r\n Accept-Encoding: identity\r\n Host: localhost:8080\r\nConnection: Keep-Alive\r\nContent-Length: 18\r\n\r\nthis was a body... ")
                .unwrap();
        assert_eq!(req.uri(), "/something/neat/here/1");
        assert_eq!(req.method(), http::Method::GET);
        let ua = req.headers().get("User-Agent");
        let expected = http::HeaderValue::from_str("Wget/1.20.1 (linux-gnu)").unwrap();
        assert_eq!(
            req.body().len(),
            req.headers()
                .get("Content-Length")
                .unwrap()
                .to_str()
                .unwrap()
                .parse::<usize>()
                .unwrap()
        );
        assert_eq!(*req.body(), "this was a body...");
        assert_eq!(ua, Some(&expected));
    }

    #[test]
    fn test_parse_request() {
        let (_, req) =
            parse_http_request("GET /something/here HTTP/1.1\r\nUser-Agent: something\r\n\r\n")
                .unwrap();
        assert_eq!(req.uri(), "/something/here");
        assert_eq!(req.method(), http::Method::GET);
        let ua = req.headers().get("User-Agent");
        let expected = http::HeaderValue::from_str("something").unwrap();
        assert_eq!(ua, Some(&expected));
    }

    #[test]
    fn test_header() {
        let h = header("User-Agent:Wget\r\n");
        assert_eq!(h, Ok(("", ("User-Agent", "Wget"))));
    }

    #[test]
    fn test_start_line() {
        let h = start_line("GET /something/here HTTP/1.1\r\n");
        assert_eq!(
            h,
            Ok((
                "",
                (http::Method::GET, "/something/here", http::Version::HTTP_11)
            ))
        );
    }

    #[test]
    fn test_start_line2() {
        let h = start_line("GET /something/neat/here/1 HTTP/1.1\r\nUser-Agent: Wget/1.20.1 (linux-gnu)\r\nAccept: */*\r\n Accept-Encoding: identity\r\n Host: localhost:8080\r\nConnection: Keep-Alive\r\nContent-Length: 15\r\n\r\n{\"an\":\"object\"}");
        assert_eq!(
            h,
            Ok((
                "User-Agent: Wget/1.20.1 (linux-gnu)\r\nAccept: */*\r\n Accept-Encoding: identity\r\n Host: localhost:8080\r\nConnection: Keep-Alive\r\nContent-Length: 15\r\n\r\n{\"an\":\"object\"}",
                (http::Method::GET, "/something/neat/here/1", http::Version::HTTP_11)
            ))
        );
    }

    #[test]
    fn test_method() {
        let parsed = get("GET");
        assert_eq!(parsed, Ok(("", http::Method::GET)));
        let parsed = post("POST");
        assert_eq!(parsed, Ok(("", http::Method::POST)));
    }

    #[test]
    fn test_uri() {
        let parsed = uri("/something/here HTTP/1.1\r\n");
        assert_eq!(parsed, Ok(("HTTP/1.1\r\n", "/something/here")));
    }

    #[test]
    fn test_version() {
        let parsed = http11("HTTP/1.1\r\n");
        assert_eq!(parsed, Ok(("", http::Version::HTTP_11)));
    }
}
