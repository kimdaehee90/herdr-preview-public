# Herdr Preview · Browser

터미널 옆에서 작업 결과를 미리 보고 웹사이트를 조작할 수 있는 Herdr 확장 버전입니다.
**PPT/PDF·여러 PNG 미리보기와 페이지 이동, Browser·Preview 탭, 스페이스/워크트리별 Browser, 한국어 글꼴과 개인 화면 배치**를 포함합니다.
원본은 [herdrdev/herdr-app](https://github.com/herdrdev/herdr-app)입니다.

## 다른 맥북에 설치

Apple Silicon(M 시리즈) · macOS 14 이상에서 사용합니다.
먼저 기본 Herdr 앱과 호환 core를 설치하세요. core는 generation 1, 0.1.11 이상이 필요합니다.
Herdr Preview가 실행 중이면 창만 닫은 뒤 아래 명령어를 터미널에 붙여넣으세요.

```bash
git clone --depth 1 https://github.com/kimdaehee90/herdr-preview-public.git "$HOME/.local/share/herdr-preview-public" && bash "$HOME/.local/share/herdr-preview-public/install-personal.sh" && open "$HOME/Applications/Herdr Preview.app"
```

GitHub 로그인 없이 설치할 수 있습니다. 설치 후 오른쪽 위의 **Preview** 또는 **Browser**를 누르세요.
기본 앱 설치 방법은 [원본 저장소 안내](https://github.com/herdrdev/herdr-app#install)를 참고하세요.
PPT/PPTX를 보려면 LibreOffice도 필요합니다. 이미 설치되어 있으면 그대로 사용하며,
없다면 [공식 LibreOffice](https://www.libreoffice.org/download/download-libreoffice/)를
응용 프로그램 폴더에 설치하세요. PDF와 PNG는 추가 변환 도구 없이 사용할 수 있습니다.

## 이미 설치했다면 업데이트

Herdr Preview 창을 닫고 실행하세요. 코어와 AI 작업은 계속 실행해 둘 수 있습니다.

```bash
git -C "$HOME/.local/share/herdr-preview-public" pull --ff-only && bash "$HOME/.local/share/herdr-preview-public/install-personal.sh" && open "$HOME/Applications/Herdr Preview.app"
```

설치기는 이전 앱과 설정을 백업하며, 기존 Browser 탭 목록과 이 맥의 로그인 프로필을 유지합니다.
코어를 교체하거나 재시작하지 않습니다.
위 새 설치·업데이트 명령 모두 저장소에 함께 배포된 최신 앱 패키지를 설치합니다.
이번 문서 미리보기·페이지 이동 기능도 같은 명령으로 적용됩니다.

## 포함한 기능

- **Preview:** PPT/PPTX 정적 슬라이드·PDF·이미지·HTML·개발 서버 결과물. `+`로 탭 추가, 클릭으로 전환, `×`로 닫기.
- **문서 페이지:** 현재/전체 페이지, 이전·다음 버튼, 번호 입력 후 Enter로 바로 이동. 기존 맞춤·확대/축소 유지.
- **여러 PNG:** Preview의 파일 선택창에서 여러 장을 고르면 선택한 목록을 순서대로 넘겨 봅니다. 탭별 이미지 목록·현재 페이지를 유지하고 앱을 다시 열 때 복원합니다.
- **Browser:** 웹사이트 탭, 로그인 유지, 클릭·한글 입력·사진 업로드.
- **작업 공간별 Browser:** 스페이스나 워크트리를 바꾸면 해당 작업의 Browser와 탭들이 표시됩니다.
- **AI별 담당 탭:** 각 AI가 자신의 스페이스·탭을 지정해 다른 편집기에 동시에 입력할 수 있습니다.
- **간결한 도구 모음:** 왼쪽 제목·앞뒤 이동, 가운데 맞춤·줌, 오른쪽 PC/모바일·블로그. 주소창은 그 아래 한 줄입니다.
- D2Coding·Pretendard 글꼴, 한국어 입력, 키보드 포커스 복원, Codex 하단 상태줄 색상.
- 상태 보고가 없는 알려진 AI도 표시하는 Agents 모듈과 사용자 수정 보존.

Browser 탭은 같은 맥의 로그인 프로필을 공유합니다. Preview 탭과 파일 서버는 작업 공간별로 관리합니다.
탭을 바꾸는 동안 페이지·작성 내용·스크롤이 유지됩니다. 앱 재실행 시에는 탭 주소·선택·보기 설정을 복원하므로, 작성 중인 글은 사이트에서 임시저장한 뒤 앱을 닫으세요.
열린 탭은 패널당 최대 32개이며, 복원된 비활성 탭은 선택할 때 불러옵니다.

## 두 작업을 동시에 맡기기

AI 패인을 두 개 열고 각각 별도 대화로 지시하세요. 예를 들면:

> A 블로그용 Browser 탭을 만들고 이 탭만 사용해서 글을 작성해줘. 결과 파일은 blog-a 폴더에 저장해줘.

> B 블로그용 Browser 탭을 만들고 이 탭만 사용해서 다른 글을 작성해줘. 결과 파일은 blog-b 폴더에 저장해줘.

Git 저장소에서는 독립 워크트리와 스페이스를 만들어 작업 파일도 분리할 수 있습니다.
AI용 상세 안내는 설치 폴더의 `AGENTS.md`와 `herdr-preview --skill`에 있습니다.
자동 조작은 담당 `space`·`tab`을 명시하며, 없는 대상을 지정하면 다른 탭으로 대체하지 않고 실패합니다.

## 폴더 접근 허용과 로그인

첫 설치 또는 기존 임시 서명에서 전환할 때 macOS가 폴더 접근 허용을 한 번 요청할 수 있습니다.
설치기는 맥마다 전용 코드 서명 인증서를 만들고 이후 업데이트에도 같은 인증서를 재사용합니다.
이전 앱과 새 앱의 서명이 달라 폴더 접근을 반복 요청하던 문제를 방지합니다.
시스템 신뢰 목록이나 전체 디스크 접근 권한은 변경하지 않습니다.

서명 키는 `~/Library/Application Support/Herdr Preview/Signing`에서 이 맥의 업데이트에만 재사용합니다.
웹사이트 로그인은 설치한 맥에서 직접 진행하세요. 발행·저장은 실제 사이트에서 확인해야 합니다.

## 설치 위치와 개인정보

| 항목 | 위치 |
|---|---|
| 앱 | `~/Applications/Herdr Preview.app` |
| 설정 | `~/.config/herdr-preview` |
| 명령 | `~/.local/bin/herdr-preview` |
| 복원 전 백업 | `~/Library/Application Support/Herdr Personal/backups/` |

공개 패키지에는 로그인 쿠키·인증키·게시글 초안·대화·코어 세션·기기 전용 서명 키를 포함하지 않습니다.
선별한 글꼴·단축키·화면 설정만 배포하며, 빌드 경로와 압축 파일 메타데이터를 일반화하고 개인정보 검사를 진행합니다.
자세한 내용은 [공개 빌드 안내](PUBLIC_RELEASE.md)를 참고하세요.

기존 오픈소스 저작자 표시는 유지합니다. 원본과 변경 소스는 Apache-2.0, 글꼴은 SIL Open Font License입니다.
[LICENSE](LICENSE) · [THIRD-PARTY-NOTICES](THIRD-PARTY-NOTICES) · [글꼴 라이선스](personal-profile/fonts/)
