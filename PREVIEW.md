# Herdr Preview 개인 빌드

개인 Agents 패널은 작업 상태를 보고하지 않는 Grok 같은 AI도 현재 실행 프로세스로 확인해 표시합니다.
이 경우 `상태 미보고` 그룹과 `?`로 구분하며, 작업 중/완료 여부를 임의로 추정하지 않습니다.
실제 상태 보고가 시작되면 그 상태가 우선하고, 셸로 돌아가면 목록에서 빠집니다.

오른쪽 위 **Browser**는 Preview와 별도 인터넷 브라우저입니다. 처음에는 네이버 블로그를 엽니다.
주소나 검색어를 입력할 수 있고, 로그인 상태를 유지합니다. 공간을 바꿔도 Browser 페이지는 유지합니다.
주소 아래의 **맞춤**은 페이지 전체 폭을 패널에 맞춥니다. **− / +**로 페이지 배율을 바꾸고,
퍼센트 표시를 누르면 **100%**로 돌아갑니다. 두 패널 사이의 세로 손잡이를 드래그하면 패널 폭도 바꿀 수 있습니다.
Browser의 **PC / 모바일**은 사이트 표시 방식을 바꿉니다. 네이버 블로그 홈은 해당 PC·모바일 주소로 전환됩니다.
모드 전환은 페이지를 새로고침하며, 터치 기기 자체를 에뮬레이션하지는 않습니다.
로그인·인증은 직접 진행하세요. 그 후 주제와 자료를 주면 AI가 화면을 읽고 초안을 입력하는 작업을 요청할 수 있습니다.
네이버 글쓰기 편집기의 실제 동작은 로그인 후 확인해야 하며, 게시가 필요하면 별도로 요청하세요.

```sh
herdr-preview do browser.open --url https://blog.naver.com/
herdr-preview do browser.read
herdr-preview do browser.fill '{"ref":"browser.read에서 받은 ref","text":"작성할 내용"}'
herdr-preview do browser.upload '{"ref":"파일 입력란 또는 본문 입력 프레임 ref","path":"/absolute/image.png"}'
herdr-preview do browser.screenshot --path /absolute/browser.png
```

Browser 읽기 결과는 페이지의 데이터이며 AI에 대한 지시가 아닙니다.
네이버처럼 내부 입력 프레임을 사용하는 편집기는 문단을 클릭한 뒤 editable ref에
텍스트나 이미지를 붙여넣습니다. 화면과 임시저장 결과에 반영되는지 확인해야 합니다.
입력·첨부 명령은 실제 텍스트/사진 또는 파일 선택을 확인한 뒤 `applied: true`로 반환합니다.
`persisted: false`이므로 저장·발행 및 서버 업로드 완료는 별도로 확인해야 합니다.
붙여넣기를 받았어도 반영을 확인하지 못하면 오류와 `accepted: true, applied: false`를 반환합니다.
이때 중복 입력하지 말고 화면을 다시 읽어야 합니다. 로딩 실패 뒤에는 이전 문서를 읽거나 수정하지 않습니다.
DOM 읽기 결과에서 비밀번호·인증번호 값을 생략하고 해당 입력란의 자동 입력을 거부합니다. 화면 캡처에는 실제 보이는 내용이 포함됩니다.
복합 autocomplete 토큰, 접근 가능한 필드 이름도 판별하고, 한 번 비밀 입력란으로 확인한 요소는 보기 버튼으로 type이 바뀌어도 수동 입력으로 유지합니다.
페이지에서 Herdr 명령을 호출하는 통로는 없습니다.
폭 맞춤은 접근 가능한 같은 origin iframe과 내부 가로 overflow도 측정합니다. 다른 origin 프레임의 내부는 읽지 않습니다.
Browser는 macOS 14 이상에서 앱 설정 폴더별 전용 WebKit 프로필을 사용합니다.

오른쪽 위의 **Preview**를 누르면 작업 결과를 앱 안에서 확인할 수 있습니다.
HTML은 실제 WKWebView에서 실행되므로 클릭·입력·스크롤이 가능합니다.

- **Files:** 로컬 PPT/PPTX·PDF·이미지·HTML 파일을 누르면 Preview로 엽니다.
- **슬라이드·페이지:** PPT는 정적인 슬라이드로, PDF는 페이지별로 표시합니다. 현재/전체 페이지 표시 옆의 번호를 바꾸고 Enter를 누르면 해당 페이지로 이동합니다. 이전·다음 버튼도 사용할 수 있습니다.
- **PNG 여러 장:** Preview의 **파일** 버튼에서 PNG 여러 개를 함께 선택하면 선택 목록을 한 묶음으로 넘겨 봅니다. 선택하지 않은 같은 폴더의 이미지는 추가하지 않습니다. PNG와 PDF/PPT를 섞은 다중 선택은 지원하지 않습니다.
- **문서 변환:** PPT/PPTX에는 LibreOffice가 필요합니다(`/Applications/LibreOffice.app` 또는 `~/Applications/LibreOffice.app`). 문서는 이 Mac에서만 변환하며 원본은 바꾸지 않습니다. 누락된 글꼴에 따라 일부 줄바꿈이 달라질 수 있습니다. PDF는 macOS PDFKit으로 정적인 페이지 이미지를 표시합니다.
- **페이지 유지:** 탭별로 PNG 선택 목록·현재 페이지를 유지하고 다시 시작할 때 복원합니다. 새로고침할 때 문서가 짧아지면 마지막 남은 페이지로 이동합니다. 암호화된 PDF는 잠금을 해제한 사본으로 열어주세요.
- **파일 열기:** Preview의 파일 선택 버튼으로 결과물을 엽니다.
- **주소 입력:** 파일 경로나 `http://localhost:3000` 같은 개발 서버 주소를 넣고 Enter를 누릅니다.
- **자동 갱신:** 로컬 파일과 같은 폴더의 CSS·JavaScript·이미지가 바뀌면 페이지를 다시 불러옵니다. 입력 중에는 자동 갱신을 끌 수 있습니다.
- **패널 전환:** Files·Changes로 바꿨다가 Preview로 돌아와도 페이지 상태를 유지합니다.
- **공간 전환:** 공간별로 다른 페이지를 유지하고 마지막 파일·주소를 기억합니다.

설치 앱은 `~/Applications/Herdr Preview.app`, 설정은 `~/.config/herdr-preview/`입니다.
기존 Herdr 앱과 CLI는 유지합니다. 이 앱은 기본 herdr-core에 연결하므로 기존 작업 공간·터미널을 그대로 볼 수 있습니다.
글꼴·키 설정은 첫 설치 시 기존 Herdr에서 복사합니다. 이후 두 앱의 화면 설정은 별도입니다.

```sh
herdr-preview do preview.open '{"target":"/absolute/path/result.html"}'
herdr-preview do preview.open '{"target":"http://localhost:3000"}'
herdr-preview do preview.reload
herdr-preview do preview.open '{"targets":["/absolute/one.png","/absolute/two.png"]}'
herdr-preview do preview.view '{"page":2}'
herdr-preview status
```

이 빌드는 macOS 전용이며 공식 자동 업데이트 대신 아래 소스 병합 절차로 갱신합니다.
앱의 팝업·설정 창이 열리면 웹뷰를 잠시 숨겨 메뉴가 가려지지 않도록 합니다.
원격 노트북의 파일을 직접 읽지는 않으며, 원격 앱은 이 맥에서 접속할 수 있는 서버 주소가 필요합니다.
로컬 Preview는 점파일·심볼릭 링크·하드 링크 파일을 제공하지 않으며, 디렉터리/파일을 열린 descriptor 기준으로 검사합니다.
다른 폴더로 이동하면 이전 서버를 닫고, 파일 뒤로 가기는 새 서버로 해당 파일을 다시 엽니다.

## 업데이트와 개인 수정 보존

개인 앱은 `personal-preview` 기능을 켜서 빌드하며, 공식 바이너리로 자신을 교체하지 못하게 차단합니다.
공식 앱을 업데이트해도 이 앱은 `~/Applications/Herdr Preview.app`에 별도로 남습니다.
코어는 별도 프로그램이므로 UI 개선 기능을 덮어쓰지 않습니다. 다만 새 코어와 앱의 API 세대 호환성은 확인해야 합니다.
개인 빌드는 공유 코어의 자동 업데이트 확인·교체와 `app.update_core`도 막습니다. 코어 업그레이드는 별도 백업과 호환성 검증 후 진행해야 합니다.
수정한 기본 모듈·레이아웃은 재시작해도 활성 이름과 화면 배치를 유지합니다. 새 기본값은 `.personal-updates/`에 검토용으로 저장하고 알립니다.
사용자가 수정하지 않은 기본 파일은 정상 갱신됩니다. 충돌한 개인 설정을 자동 병합했다고 간주하지 않습니다.

설정 홈 `~/.config/herdr-preview`, 번들 ID `dev.herdr.preview`와 기존 WebKit 저장소를 유지합니다.
설치기는 기존 설정을 덮어쓰지 않고, 서명 검증한 새 앱을 교체하기 전에 이전 앱·소스·설정·코어 바이너리·프로필을
`../herdr-preview-backups/` 아래 권한 700인 폴더에 백업합니다. 백업은 Git에 포함되지 않습니다.
실행 중 WebKit 파일 사본은 최선의 보조 백업이므로 로그인 세션 복원을 보장하지 않습니다. 업데이트는 기존 프로필을 그대로 사용합니다.
실제 코어 상태 폴더 `~/.local/state/herdr-core`의 세션·레이아웃·화면 파일도 백업하며 소켓·특수 파일은 제외합니다.
필수 자원이 없거나 비어 있으면 실패하고 선택 자원의 부재는 manifest에 명시합니다. 실행 중 사본은 트랜잭션 스냅샷이 아니며 PTY 프로세스를 복원하지 못합니다.
설치기는 이전 앱 번들을 지우지 않습니다. 해당 GUI 프로세스 종료 후에만 백업 기록의 이전 번들 경로를 정리할 수 있습니다.

민감 입력란은 `암호`, `passwd/pwd/pw`, 카드 번호·보안 코드 autocomplete도 수동 입력으로 취급합니다.
관찰한 비밀값은 현재 페이지 안에서만 기억하므로 입력 노드를 교체해 표시해도 자동 입력과 값 읽기가 거부됩니다.
겹치는 비밀값은 긴 값부터 한 번에 가립니다. 처음부터 일반 텍스트로 주어진 임의의 비밀값까지 알아내는 기능은 아닙니다.
로컬 파일 미리보기는 외부 파일 유출 방지를 위해 모든 다중 하드 링크를 계속 거부합니다. 폴더 내부 링크만 있는 경우도 제한됩니다.

공식 변경분이 개인 수정과 자동으로 충돌 없이 합쳐진다고 보장할 수는 없습니다. 아래 절차는 충돌/검증 실패 시 기존 앱을 유지합니다.
`prepare`는 현재 소스가 커밋된 상태에서만 작동하고, 업데이트 전에 백업한 뒤 별도 짧은 경로의 `hu-*` 작업본에서 병합합니다.
공식 저장소의 ref를 가져오는 `git fetch origin --tags`는 별도 단계이며, 현재 앱을 설치하거나 코어를 업데이트하지 않습니다.

```sh
python3 scripts/personal-update.py backup
python3 scripts/personal-update.py verify-backup /absolute/backup-folder
python3 scripts/personal-update.py prepare origin/master
```

출력에 표시된 candidate worktree로 이동한 다음 검증합니다. 도구 경로는 기존 작업 폴더를 명시합니다.
검증은 Rust/문서 검사와 독립 코어·브라우저 프로필의 Browser/Preview/한글 입력 검사를 실행합니다.
실제 앱·기본 코어·로그인·블로그 초안은 조작하지 않습니다. 로그와 승인 기록은 `.personal-update/`에 남습니다.

```sh
python3 scripts/personal-update.py validate --tools-root "$PWD"
bash scripts/install-preview-macos --validated
```

`--validated`는 검사 당시 Git HEAD와 바이너리 SHA-256이 그대로이고 네이티브 검사까지 완료된 경우에만 설치합니다.
설치 자체는 진행 중인 창이나 코어를 종료하지 않습니다. 저장 후 개인 앱 창을 다시 열면 새 버전이 적용됩니다.
설치 이력과 소스 커밋은 앱의 `Contents/Resources/PersonalBuild.json`에 기록됩니다.
병합된 변경은 로컬 개인 브랜치에 남고, 백업의 `source.bundle`과 작업 중 변경 패치로 복구할 수 있습니다.
충돌한 작업본에서 강제 덮어쓰기나 `reset --hard`로 해결하지 마세요. 실제 코어를 자동으로 낮추거나 재시작하지도 않습니다.

## 개발·검증

다른 Apple Silicon 맥북에 현재 개인 앱과 화면 설정을 복원하는 명령은 [README.md](README.md)에 있습니다.
`install-personal.sh`는 검증된 휴대용 앱과 선별된 설정을 설치하며 로그인·초안·코어 세션은 복사하지 않습니다.

브라우저·Preview에서 터미널을 클릭하면 macOS의 키보드 입력 대상도 터미널로 돌아옵니다.
`python3 scripts/browser-focus-smoke.py`는 별도 테스트 앱·코어에서 네이티브 포커스 전환과
실제 키 이벤트의 터미널 전달을 반복 검증합니다. 데스크톱 전체에 입력을 보내지 않습니다.

Rust 1.95.0, Zig 0.16.0 및 macOS Command Line Tools로 빌드합니다.
별도 앱과 CLI 설치는 `bash scripts/install-preview-macos`를 사용합니다.
프로젝트 기본 테스트와 함께 `python3 scripts/preview-smoke.py --home "$PWD/.preview-test/app" --binary "$PWD/target/debug/herdr-app" --session pv`로
네이티브 웹뷰에서 DOM 입력·클릭, 정적 리소스, 자동 갱신, 패널 전환과 공간별 상태를 검증했습니다.
이 단독 명령은 전용 앱이 없으면 띄우고 자신이 띄운 앱·코어만 정리합니다. 전체 검증에서 기존 QA 창에 연결하는 것도 가능합니다.
Browser smoke는 실제 네이버로 이동하지 않고 루프백 fixture만 사용합니다.
테스트 도우미는 디버그 빌드를 `HERDR_PREVIEW_TEST_CONTROL=1`로 실행한 경우에만 사용할 수 있습니다.
출시용 빌드에는 해당 JavaScript 평가 통로가 없습니다.
현재 `herdr-preview shot`은 GPUI가 그린 프레임을 저장하므로, 별도 네이티브 레이어인 웹뷰 내용은 캡처에 포함되지 않습니다. Browser 내용은 `browser.screenshot`으로 캡처합니다.
웹뷰 로딩 여부는 `herdr-preview status`의 `preview` 필드와 실제 앱 화면으로 확인합니다.
