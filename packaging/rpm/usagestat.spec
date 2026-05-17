Name:           usagestat
Version:        1.0.1
Release:        1%{?dist}
Summary:        Scriptable CLI for local agent usage data

License:        MIT
URL:            https://github.com/Hashim-K/usagestat
Source0:        %{url}/archive/refs/tags/v%{version}.tar.gz

BuildRequires:  cargo
BuildRequires:  gcc
BuildRequires:  openssl-devel
BuildRequires:  pkgconfig
BuildRequires:  rust

%description
usagestat is a scriptable CLI for probing and exporting local agent usage data.

%prep
%autosetup -n usagestat-%{version}

%build
cargo build --release --locked -p usagestat-cli

%install
install -Dm0755 target/release/usagestat %{buildroot}%{_bindir}/usagestat

%check
cargo test --locked -p usagestat-cli

%files
%license LICENSE
%{_bindir}/usagestat

%changelog
* Mon May 18 2026 Hashim-K <Hashim-K@users.noreply.github.com> - 1.0.1-1
- Install rustls ring crypto provider before plugin HTTP requests

* Sat May 16 2026 Hashim-K <Hashim-K@users.noreply.github.com> - 1.0.0-1
- Initial RPM package
