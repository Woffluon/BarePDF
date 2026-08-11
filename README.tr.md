# BarePDF

> Windows için hızlı, özel ve sade PDF okuma.

[English README](./README.md) · **Türkçe**

BarePDF; Windows 10 ve 11 için Rust, Slint ve PDFium ile geliştirilen açık kaynaklı PDF okuyucudur. Belgeler yerel kalır: hesap, reklam, telemetri ve bulut yüklemesi yoktur. Görüntüleme ve önbellekler byte bütçeleriyle sınırlandırıldığı için uzun belgelerde bellek kullanımı öngörülebilir kalır.

## Neden BarePDF?

- **Hızlı:** Görünen sayfalara öncelik verilir, eski işler rasterleştirilmeden iptal edilir.
- **Sınırlı kaynak:** Ham görüntü, sayfa ve küçük resim önbellekleri ayrı byte bütçelerine sahiptir.
- **Çevrimdışı öncelik:** PDF okumak için hesap, analitik veya ağ bağlantısı gerekmez.
- **Windows’a uyumlu:** Kurulum, Birlikte Aç, dosya bırakma, pano ve yüksek DPI akışları yereldir.
- **Denetlenebilir sürüm:** Güncelleme bildirimi Ed25519 imzası, URL, boyut, SHA-256 ve sürüm ile doğrulanır.

## İndirme

Güncel paketleri [resmi indirme sayfasından](https://woffluon.github.io/BarePDF/download/) alın.

| İhtiyaç | Dosya |
| --- | --- |
| Normal kurulum | BarePDF-Setup-x64-vX.Y.Z.exe |
| Kurulumsuz kullanım | BarePDF-Portable-x64-vX.Y.Z.zip |
| İndirme doğrulama | BarePDF-vX.Y.Z-SHA256SUMS.txt |

> [!IMPORTANT]
> Kurulum dosyası Authenticode ile imzalanmaz; Windows bilinmeyen yayımcı uyarısı gösterebilir. Yalnızca resmi siteden veya [GitHub Releases](https://github.com/Woffluon/BarePDF/releases/latest) üzerinden indirin.

## Hızlı başlangıç

### Kurulum paketi

1. İndirme sayfasını açın ve BarePDF-Setup-x64-vX.Y.Z.exe dosyasını çalıştırın.
2. Gerekirse Windows Varsayılan Uygulamalar ekranından BarePDF’i .pdf için seçin.
3. Ctrl+O, dosya bırakma veya Dosya Gezgini ile PDF açın.

### Taşınabilir sürüm

ZIP arşivini yazılabilir bir klasöre çıkarın ve BarePDF.exe dosyasını çalıştırın. Kurulum veya kayıt defteri değişikliği gerekmez.

## Özellikler

- Sürekli dikey ve tek sayfa okuma.
- Sınırlandırılmış sayfa numarası, yakınlaştırma ve yönlendirme kontrolleri.
- Metin seçimi, pano kopyalama ve arama.
- F11 tam ekran, F5 sunum modu ve açık/koyu tema.
- PDFium tabanlı metin, yazı tipi ve görüntü rasterleştirme.
- Windows dosya bırakma, Varsayılan Uygulamalar ve yüksek DPI desteği.
- Hesap, reklam, analitik, bulut senkronizasyonu veya belge yükleme yok.

## Sistem gereksinimleri

- Windows 10 veya Windows 11, 64 bit.
- Belge boyutuna göre yeterli RAM ve yaklaşık 100 MB boş disk alanı.
- Kaynaktan derleme için Rust 1.92 veya daha yeni sürüm, Git ve PowerShell.

## Güncellemeler ve güvenlik

Güncelleme denetimleri varsayılan olarak kapalıdır. Kullanıcı izin verirse ağ isteği arka planda yapılır. Yükleme öncesi HTTPS adresi, imza, sürüm, boyut ve SHA-256 doğrulanır; sessiz kurulum, sürüm düşürme veya güvenilmeyen yönlendirme kabul edilmez. Taşınabilir sürüm yalnızca sürüm sayfasına bildirim verir.

## Klavye kısayolları

| Kısayol | İşlem |
| --- | --- |
| Ctrl+O | PDF aç |
| Ctrl+F | Metin ara |
| Ctrl+C | Seçili metni kopyala |
| Ctrl+Plus / Ctrl+Minus | Yakınlaştır / uzaklaştır |
| Ctrl+0 | Sığdırma moduna dön |
| F5 | Sunum modu |
| F11 | Tam ekran |
| Esc | Özel modu kapat |
| PageUp / PageDown | Önceki / sonraki sayfa |

## Geliştirici rehberi

~~~powershell
rustup toolchain install 1.92.0
rustup default 1.92.0
git clone https://github.com/Woffluon/BarePDF.git
Set-Location BarePDF
cargo run --release --package barepdf
~~~

PDFium ikilisini yalnızca imzalı ve checksum allowlist’i bulunan CI akışından alın. Rastgele native binary eklemeyin.

Web sitesini yerelde çalıştırmak için:

~~~powershell
pnpm --dir website install
pnpm --dir website run dev
~~~

Site, İngilizce ve Türkçe arasında erişilebilir bir dil seçiciyle geçiş yapar; Türkçe metinler Windows kullanım bağlamına göre yerelleştirilmiştir.

## Mimari

- apps/barepdf: Windows süreci, Slint olayları, updater ve uygulama durumu.
- crates/barepdf-core: domain tipleri, tercihler, seçim ve hata modeli.
- crates/barepdf-pdf: PDFium adaptörü ve kontrollü PDF sınırları.
- crates/barepdf-render: worker, önceliklendirme, cache ve event kanalları.
- crates/barepdf-platform*: Windows dosya, pano ve pencere entegrasyonu.
- crates/barepdf-ui: Slint görünüm tanımları.
- website: Astro tabanlı site ve GitHub release görünümü.

## Test

~~~powershell
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked
cargo build --release --locked
cargo audit
pnpm --dir website run test
pnpm --dir website exec astro check
pnpm --dir website run build
~~~

Güvenlik veya paketleme değişikliklerinde Windows staging ve installer doğrulamasını da çalıştırın.

## Windows paketleri

~~~powershell
powershell -File packaging/windows/scripts/stage-release.ps1
powershell -File packaging/windows/scripts/build-portable.ps1
powershell -File packaging/windows/scripts/build-installer.ps1
powershell -File packaging/windows/scripts/validate-installer.ps1
~~~

Ürün sürümü kök Cargo.toml içindeki workspace.package.version alanından gelir. Commit öncesi prepare-version.ps1 yardımcı programını, Conventional Commit mesajıyla çalıştırın.

## Katkıda bulunma

1. Küçük ve bağımsız bir değişiklik seçin.
2. Gerekli regression testini ekleyin.
3. AGENTS.md kurallarını ve güvenlik sınırlarını izleyin.
4. Format, Clippy, test ve gerekli web/paketleme kontrollerini çalıştırın.
5. Açıklayıcı bir Conventional Commit gönderin.

## Gizlilik, güvenlik ve lisans

BarePDF belgeleri yerel işler; varsayılan ağ davranışı yoktur. Güncelleme denetimi kullanıcı seçimine bağlıdır. Yalnızca HTTPS GitHub/BarePDF release uçları kabul edilir. Native binary, imza anahtarı veya token repoya eklenmez.

BarePDF [MIT Lisansı](./LICENSE) ile yayımlanır. Üçüncü taraf bildirimleri THIRD_PARTY_NOTICES.md dosyasındadır.

Dil seçimi: [English README](./README.md) · [Türkçe README](./README.tr.md)
