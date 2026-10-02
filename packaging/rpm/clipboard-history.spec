Name:           clipboard-history
Version:        0.1.0
Release:        1%{?dist}
Summary:        High-performance Universal Clipboard History utility for Linux

License:        MIT or Apache-2.0
URL:            https://github.com/alierenaltindag/clipboard-history
Source0:        %{name}-%{version}.tar.gz

BuildRequires:  cargo
BuildRequires:  rust
BuildRequires:  gcc
BuildRequires:  pkgconfig
BuildRequires:  libX11-devel
BuildRequires:  libXtst-devel

Requires:       libX11
Requires:       libXtst
Recommends:     wl-clipboard
Recommends:     xdotool

%description
Modern Win+V clipboard alternative for Wayland and X11 desktop environments,
supporting rich text, images, file lists, and instant fuzzy search.

%prep
%autosetup

%build
cargo build --release --workspace

%install
rm -rf $RPM_BUILD_ROOT
mkdir -p $RPM_BUILD_ROOT%{_bindir}
mkdir -p $RPM_BUILD_ROOT%{_datadir}/applications
mkdir -p $RPM_BUILD_ROOT%{_datadir}/icons/hicolor/scalable/apps
mkdir -p $RPM_BUILD_ROOT%{_userunitdir}

install -m 755 target/release/clipboard-history $RPM_BUILD_ROOT%{_bindir}/
install -m 755 target/release/clipboard-history-daemon $RPM_BUILD_ROOT%{_bindir}/
install -m 755 target/release/clipboard-history-gui $RPM_BUILD_ROOT%{_bindir}/

install -m 644 packaging/desktop/clipboard-history.desktop $RPM_BUILD_ROOT%{_datadir}/applications/
install -m 644 packaging/desktop/icons/clipboard-history.svg $RPM_BUILD_ROOT%{_datadir}/icons/hicolor/scalable/apps/
cp -r packaging/desktop/icons/hicolor/* $RPM_BUILD_ROOT%{_datadir}/icons/hicolor/
install -m 644 packaging/systemd/clipboard-history.service $RPM_BUILD_ROOT%{_userunitdir}/

%files
%{_bindir}/clipboard-history
%{_bindir}/clipboard-history-daemon
%{_bindir}/clipboard-history-gui
%{_datadir}/applications/clipboard-history.desktop
%{_datadir}/icons/hicolor/*/apps/clipboard-history.*
%{_userunitdir}/clipboard-history.service

%changelog
* Fri Oct 02 2026 Ali Eren Altındağ <alierenaltindaag@gmail.com> - 0.1.0-1
- Initial release
