# Maintainer: Your Name <your@email.com>
pkgname=pdf-manager
pkgver=0.1.0
pkgrel=1
pkgdesc="Merge, reorder, rotate and manage PDF documents"
arch=('x86_64')
license=('MIT')
depends=('poppler-glib' 'cairo' 'gtk3')
makedepends=('rust' 'cargo' 'pkg-config')
source=()

build() {
    cd "$startdir"
    cargo build --release --locked
}

package() {
    cd "$startdir"

    # Binary
    install -Dm755 "target/release/$pkgname" "$pkgdir/usr/bin/$pkgname"

    # Desktop entry
    install -Dm644 "packaging/$pkgname.desktop" "$pkgdir/usr/share/applications/$pkgname.desktop"

    # Icon
    install -Dm644 "ui/images/Logo.png" "$pkgdir/usr/share/icons/hicolor/1024x1024/apps/$pkgname.png"

    # License
    install -Dm644 "LICENSE" "$pkgdir/usr/share/licenses/$pkgname/LICENSE"
}
