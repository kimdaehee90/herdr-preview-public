const input = document.getElementById('name');
input.addEventListener('input', () => {
  document.getElementById('greeting').textContent = input.value ? `${input.value}님, 화면 안에서 바로 작업하세요.` : '이름을 입력하면 이곳에 표시됩니다.';
});
let count = 0;
document.getElementById('counter').addEventListener('click', () => {
  document.getElementById('count').textContent = String(++count);
});
