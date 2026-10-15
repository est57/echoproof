use std::path::Path;

use crate::cli::Runtime;

/// Process settings that route a test build's outbound calls through the proxy and make it
/// trust the Echoproof CA, without touching the system trust store.
pub fn lines(runtime: Runtime, ca: &Path, proxy: &str) -> String {
    let ca = ca.display();
    let common = format!("export HTTPS_PROXY=http://{proxy}\nexport HTTP_PROXY=http://{proxy}\n");
    match runtime {
        Runtime::Php => format!(
            "{common}# Pass these to the PHP process of the test build:\n\
             #   php -d curl.cainfo={ca} -d openssl.cafile={ca} ...\n\
             # PHP-FPM drops the environment by default; add to the test pool:\n\
             #   env[HTTPS_PROXY] = http://{proxy}\n\
             #   php_admin_value[curl.cainfo] = {ca}\n\
             #   php_admin_value[openssl.cafile] = {ca}\n\
             # Under PHP-FPM, HTTP_PROXY is ignored (httpoxy), so plain http:// calls are not seen.\n"
        ),
        Runtime::Node => format!(
            "{common}export NODE_USE_ENV_PROXY=1\nexport NODE_EXTRA_CA_CERTS={ca}\n\
             # Needs Node 22.21 or 24 for fetch and node:http to honor the proxy.\n"
        ),
        Runtime::Python => {
            format!("{common}export REQUESTS_CA_BUNDLE={ca}\nexport SSL_CERT_FILE={ca}\n")
        }
        Runtime::Go => format!("{common}export SSL_CERT_FILE={ca}\n"),
    }
}
