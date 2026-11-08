use reqwest::Url;

pub(super) fn commit_url(remote: &str, commit: &str) -> Option<String> {
    let parsed = Url::parse(remote)
        .ok()
        .filter(|url| url.has_host())
        .or_else(|| {
            let (host, path) = remote.split_once(':')?;
            let host = host.rsplit_once('@').map_or(host, |(_, host)| host);
            Url::parse(&format!("ssh://{host}/{path}")).ok()
        })?;
    let host = parsed.host_str()?;
    let prefix = match host {
        "github.com" => "commit",
        "gitlab.com" => "-/commit",
        "bitbucket.org" => "commits",
        _ => return None,
    };
    let repository = parsed.path().trim_end_matches('/').trim_end_matches(".git");
    if repository.is_empty() || repository == "/" {
        return None;
    }
    let mut url = Url::parse(&format!("https://{host}")).ok()?;
    url.set_path(&format!("{repository}/{prefix}/{commit}"));
    Some(url.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hosted_commit_links_accept_ssh_and_https_without_credentials() {
        assert_eq!(
            commit_url("git@github.com:owner/repo.git", "abc").as_deref(),
            Some("https://github.com/owner/repo/commit/abc")
        );
        assert_eq!(
            commit_url("https://user:secret@gitlab.com/team/sub/repo.git", "abc").as_deref(),
            Some("https://gitlab.com/team/sub/repo/-/commit/abc")
        );
        assert_eq!(commit_url("file:///some/repo", "abc"), None);
        assert_eq!(commit_url("ssh://git@unknown.example/repo", "abc"), None);
    }
}
