# Herdr Preview · Browser

Apple Silicon 맥북에서 Preview와 Browser를 함께 사용하는 Herdr 개인 확장 빌드입니다.
원본은 [herdrdev/herdr-app](https://github.com/herdrdev/herdr-app)이며 이 저장소는 독립 배포본입니다.
기존 원본 앱과 별도로 **Herdr Preview**를 설치합니다.

## 설치

Apple Silicon · macOS 14 이상 · Git/Python 3 · 설치된 공식 Herdr/core가 필요합니다.
호환 코어는 generation 1, 0.1.11 이상입니다. 설치기는 코어를 교체하거나 재시작하지 않습니다.
저장소가 공개돼 있어 GitHub 계정이나 로그인 없이 내려받을 수 있습니다.

```sh
git clone --depth 1 https://github.com/kimdaehee90/herdr-preview-public.git "$HOME/.local/share/herdr-preview-public" && bash "$HOME/.local/share/herdr-preview-public/install-personal.sh" && open "$HOME/Applications/Herdr Preview.app"
```

이미 내려받았다면 해당 폴더에서 `git pull --ff-only` 후 `bash install-personal.sh`를 실행합니다.
복원 전에는 Herdr Preview 창만 닫으세요. 실행 중인 개인 GUI가 있으면 설정 변경을 중단합니다.
기본 Herdr 앱의 설치 방법은 [원본 저장소 안내](https://github.com/herdrdev/herdr-app#install)를 참고하세요.

## 포함한 기능

- 터미널 옆에서 HTML·이미지·PDF·개발 서버 결과물을 여는 Preview.
- 별도 Browser, 웹사이트 로그인, 클릭·한글 입력·사진 업로드, 폭 맞춤·줌·모바일 사이트 모드.
- 브라우저에서 터미널로 돌아올 때 키보드 포커스 복원.
- 비밀번호·인증번호·카드 입력란 보호와 입력 반영 확인.
- 상태 보고가 없는 알려진 AI 프로세스도 표시하는 개인 Agents 모듈.
- D2Coding·Pretendard 글꼴, 단축키, 개인 화면 배치, 업데이트 시 사용자 수정 보존.

새 로컬 macOS GUI 패인은 `NO_COLOR` 변수를 완전히 제거합니다. 빈 문자열로만 바꾸면 Codex 하단 상태줄의 색상이 여전히 꺼지는 문제까지 처리합니다.
이미 실행 중인 프로그램의 환경은 유지하므로, 기존 프로그램은 작업을 마친 뒤 새 GUI 패인에서 다시 열어야 색상이 적용됩니다.

웹사이트 로그인은 설치한 맥북에서 직접 진행합니다. 공개 패키지에 쿠키·계정 비밀값·게시글 초안·대화·코어 세션은 포함하지 않습니다.
프로그램은 `~/Applications/Herdr Preview.app`, 설정은 `~/.config/herdr-preview`, 명령은 `~/.local/bin/herdr-preview`입니다.
기존 개인 앱과 설정은 `~/Library/Application Support/Herdr Personal/backups/`에 보관하며 설치 오류 시 복구합니다.
앱 아카이브와 설정 파일의 체크섬은 `packages/personal-release.json`에 있습니다.

## 공개 배포와 소스

개인 Git 이력 없이 시작한 소스 사본을 배포합니다. Rust 컴파일 경로를 일반화하고 심볼을 제거한 후 네이티브 검사를 실행합니다.
압축 파일의 UID/GID·사용자/그룹명·시각 메타데이터를 정규화합니다.
개인 설정 중 글꼴·단축키·화면 구조에 필요한 파일만 선별하며 배포 전 개인정보·인증키 패턴 검사를 통과해야 합니다.
자세한 빌드·검증 방법은 [PUBLIC_RELEASE.md](PUBLIC_RELEASE.md)를 참고하세요.

공개 저장소의 GitHub 소유자 이름과 기존 오픈소스 저작자 표시는 공개 정보입니다.
원본과 변경 소스는 Apache-2.0, 포함한 글꼴은 각각의 SIL Open Font License를 유지합니다.
[LICENSE](LICENSE) · [THIRD-PARTY-NOTICES](THIRD-PARTY-NOTICES) · [글꼴 라이선스](personal-profile/fonts/)
