# cross-verify 스크립트 공용 함수.
# 실행 로그는 로그인한 사람만 받을 수 있지만 주석(annotation)은 공개 저장소의 check-run API 로 읽을 수 있어서,
# 실패 원인과 측정값을 주석으로 남긴다.

# escape : 표준 입력을 주석 메시지 하나로 만든다 (CR 제거, % · 줄바꿈 이스케이프, 주석 한도 4 KB 안쪽으로 자름)
escape() {
  tr -d '\r' | sed 's/%/%25/g' | awk '{printf "%s%%0A", $0}' | cut -c1-3800
}

# annot <제목> <로그> : 컴파일 · 빌드 스크립트 · 테스트 오류 줄과 로그 끝을 주석 하나로 올린다
annot() {
  local msg
  msg="$( {
    grep -E '^(error|warning: build failed)|panicked at|failed to run custom build|--- stderr|nvcc|fatal|^failures:|FAILED|exit code|STATUS_|not found' -A3 "$2" | cut -c1-300 | head -30
    echo '--- last lines ---'
    tail -25 "$2" | cut -c1-300
  } | escape )"
  echo "::error title=$1::$msg"
}
