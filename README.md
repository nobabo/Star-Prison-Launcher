# 별도소 런처

[English](README.en.md) | 한국어

별도소 런처는 별도소 Minecraft 서버 접속을 위해 제작된 Windows-first Tauri v2 런처입니다. Microsoft 계정 로그인, Java 21 런타임 설치, Fabric/Minecraft 실행 환경 구성, 서버 파일 동기화, 게임 실행까지 한 앱에서 처리합니다.

이 문서는 플레이어, 서버 운영자, 배포 담당자, 개발자가 모두 처음 확인할 수 있는 입구 문서입니다. 더 자세한 명령어와 Rust 구조 설명은 `.docs` 문서를 함께 봐 주세요.

## 빠른 시작

### 플레이어

1. 최신 설치 파일 `star-prison.exe`를 실행합니다.
2. 런처에서 Microsoft 계정으로 로그인합니다.
3. `게임 시작`을 누르면 필요한 Java, Minecraft/Fabric 파일, 서버 파일을 자동으로 내려받고 실행합니다.
4. 게임이 바로 종료되거나 리소스가 이상하면 설정 탭의 `설치 경로`에서 `로그`, `스크린샷`, `프로필`을 열어 확인합니다.

### 개발자

```bash
corepack enable
pnpm install
pnpm dev
```

WSL에서 Windows Node/Rust 경로를 자동으로 붙여 실행하려면:

```bash
./run.sh
```

전체 명령어 목록은 [.docs/commands.md](.docs/commands.md)를 확인하세요.

## 주요 기능

- Microsoft OAuth2 PKCE 로그인과 Minecraft 소유권/profile 검증
- Windows DPAPI 기반 인증 세션 보호 저장
- Java 21 런타임 자동 다운로드 및 SHA-256 검증
- Fabric/Minecraft 라이브러리, natives, assets 병렬 다운로드
- 서버 release archive 설치와 checksum 검증
- `options.txt` 기본값 병합과 사용자 설정 보존
- `mods`, `config`, `shaderpacks` 설치 상태 기록
- 설정 탭에서 프로필, 로그, 스크린샷 위치 열기
- HTTPS 및 allowlist 기반 외부 링크/다운로드 제한
- NSIS 설치 파일 생성

## 런처 데이터 경로

Windows에서 런처 데이터 루트는 기본적으로 다음 경로입니다.

```text
%APPDATA%\star-prison-launcher
```

주요 항목:

- `user-config.json`: 사용자 설정과 로그인 세션 저장 위치
- `install-state.json`: 런처 설치/실행 상태 기록
- `config/`: 첫 실행 시 seed되는 앱/서버 설정, GitHub 배포 manifest 캐시와 release archive 상태 파일
- `profile/`: 실제 Minecraft 프로필 루트
- `profile/logs/`: Minecraft 로그
- `profile/screenshots/`: Minecraft 스크린샷
- `data/runtime/java-21/`: 관리형 Java 21 런타임
- `data/downloads/`, `data/staged/`, `data/runtime/staged/`: 다운로드/설치 중 임시 캐시

정상 설치가 확인된 다운로드 zip이나 staged 폴더는 비어 있으면 정리됩니다. `profile`은 `data` 밖의 런처 루트에 두어 실제 Minecraft 프로필 경로가 과도하게 길어지지 않도록 유지합니다.

## 사용자 데이터 보존 정책

런처는 사용자 설정을 런타임마다 무조건 덮어쓰지 않습니다.

- `app.config.json`, `client.config.json`, `server.manifest.json`은 첫 실행 시 기본값으로 seed되며, 이미 존재하면 로컬 파일을 우선 사용합니다.
- `distribution.json`은 `app.config.json`에 고정된 GitHub URL에서 실행 시마다 갱신하며, 네트워크 오류 시 마지막으로 검증된 캐시와 내장 기본값을 순서대로 사용합니다.
- `options.txt`는 파일 전체 교체가 아니라 필요한 key 단위 병합을 기준으로 합니다.
- `config.zip`, `shaderpacks.zip`처럼 사용자가 직접 바꿀 수 있는 파일은 기존 파일을 보존하는 방향으로 설치됩니다.
- `mods.zip`은 서버가 관리하는 모드 구성을 맞추기 위한 release archive로 취급합니다.

런처 기본값을 바꿀 때는 `config/client.config.json`, `config/app.config.json`, `config/server.manifest.json`을 확인하세요. 모드·게임 설정·쉐이더 배포 정보는 GitHub의 `config/distribution.json`을 갱신합니다.

## 보안과 신뢰성

- 인증 토큰은 Windows에서 DPAPI로 보호 저장합니다.
- DPAPI 복호화에 실패하면 설정 전체를 지우지 않고 로그인 세션만 초기화합니다.
- 외부 링크는 HTTPS와 브라우저 allowlist를 통과해야 열립니다.
- 다운로드와 원격 JSON 요청은 HTTPS 및 다운로드 allowlist를 통과해야 합니다.
- 다운로드 파일은 가능한 경우 size와 SHA-256으로 검증합니다.
- zip 해제는 경로 탈출, 비정상 크기, 과도한 파일 수를 검사합니다.
- 위험한 JVM/Game 인자는 저장 전 경고만 표시하고 사용자가 선택할 수 있게 둡니다.
- `launcherCompanion` API는 기본 비활성입니다. 활성화하려면 `config/app.config.json`에서 `enabled`, `apiBaseUrl`이 유효해야 하며 bearer token은 로컬 비추적 설정으로만 공급해야 하며, 이벤트 제출 시 로그인 상태가 필요합니다.

## 개발 명령어 요약

```bash
corepack enable
pnpm install --frozen-lockfile
pnpm dev
pnpm lint
pnpm format:check
```

Windows 릴리즈:

```bat
release.bat
```

릴리즈 결과물:

```text
dist\star-prison.exe
```

Windows Rust 검증:

```bash
cargo fmt --check --manifest-path src-tauri/Cargo.toml
cargo check --locked --manifest-path src-tauri/Cargo.toml
```

## 문서

- [.docs/commands.md](.docs/commands.md): 명령어 상세 설명
- [.docs/rust_source_guide.md](.docs/rust_source_guide.md): Rust/Tauri 백엔드 구조
- [.docs/CHANGELOG.md](.docs/CHANGELOG.md): 변경 이력
- [.docs/AGENTS.md](.docs/AGENTS.md): 프로젝트 작업 규칙과 구현 상태

## 문제 해결

- 설치 파일 빌드가 실패하고 기존 exe를 지울 수 없다면 실행 중인 런처를 먼저 종료하세요.
- 디스크 공간 오류가 나면 `src-tauri/target` 정리 또는 `cargo clean`을 검토하세요.
- WSL에서 Tauri/Rust 검증이 Linux 의존성으로 실패하면 Windows Cargo 경로에서 다시 확인하세요.
- 게임이 실행 직후 종료되면 설정 탭의 `로그` 버튼으로 `profile/logs`를 열고 최신 Minecraft 로그를 확인하세요.
- 하드 크래시는 `profile/crash-reports`를 우선 확인하세요.
