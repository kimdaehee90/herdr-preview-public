# 공개 배포 빌드

일반 `cargo build` 결과에는 빌드한 맥의 사용자 경로가 남을 수 있습니다. 공개 패키지는 반드시 경로를 일반화해 만듭니다.

```sh
export RUSTFLAGS="--remap-path-prefix=$HOME=/build"
export CARGO_PROFILE_RELEASE_STRIP=symbols
cargo build --release --locked --features personal-preview
```

Rust 1.95.0과 Zig 0.16.0을 사용합니다. `ZIG`로 Zig 실행 경로를 지정할 수 있습니다.
코어는 별도로 설치한 호환 버전을 사용합니다. 실제 사용자의 창·기본 코어·프로필을 테스트에서 조작하지 않습니다.
빌드가 끝나면 독립 `HERDR_APP_HOME`과 짧은 `HC_SESSION`으로 네이티브 Browser 검사를 실행하고,
소스 커밋과 바이너리 SHA-256, 성공 여부를 `.public-build/validated.json`에 기록합니다.

```json
{
  "head": "PUBLIC_SOURCE_COMMIT",
  "binary_sha256": "VALIDATED_BINARY_SHA256",
  "native_ui_checks_passed": true,
  "paths_remapped": true
}
```

```sh
python3 scripts/package-personal.py
python3 scripts/audit-public.py
python3 scripts/test-personal-restore.py
```

감사 도구는 Git에 올릴 파일과 앱 압축 파일 내부를 검사합니다. 실제 배포자의 사용자명·이메일 등은
Git에 저장하지 않고 로컬 전용 `PUBLIC_PRIVACY_TERMS` 환경 변수(JSON 배열)로 추가할 수 있습니다.
기본 검사는 토큰/개인 키 패턴, 실제 사용자 홈 경로, 개인 상태 폴더와 압축 메타데이터를 확인합니다.
패키지를 다시 만들면 설치·체크섬·서명과 실제 네이티브 기능을 다시 검증하세요.

GitHub에는 검사한 소스와 패키지만 올립니다. `.public-build`, 테스트 홈, 로그, 쿠키, 작업 기록은 배포하지 않습니다.
새 Git 이력을 만들 때 로컬 실명·컴퓨터 이름이 든 커밋 이메일을 사용하지 마세요。
