# Fayger 빠른 시작

**Fayger**는 Fay(`iFay` 및 `coFay`)를 위한 멀티플랫폼 가상 실행 환경(Virtual Runtime Environment)입니다. 이식 가능한 **BuF(Built-up Fay)** 아티팩트와 선언형 **FayManifest** 조립 명세를 입력받아 데스크톱, 서버, 브라우저 및 임베디드 호스트 환경 전반에서 일관된 검증, 수명 주기 실행, 권한 범위가 제한된 호스트 어댑터 연동을 제공합니다.

> 용어 기준: **호스트(Host)**는 오직 단말 기기나 애플리케이션 프로세스만을 의미하며, 자연인은 항상 **Human Prime**으로 지칭합니다.

## 3계층 핵심 아키텍처

1. **로더 계층(Loader Layer)**: 5세그먼트 `BuF` 아티팩트를 파싱하고 해시 요약, Ed25519 서명 및 인터페이스 버전을 검증합니다.
2. **런타임 계층(Runtime Layer)**: 언어 중립적인 `Runtime_Interface` 수명 주기와 리소스 쿼터를 관리하며 `fayger-conformance` 블랙박스 테스트 스위트로 구현 간 동등성을 검증합니다.
3. **어댑터 계층(Adapter Layer)**: `Universal_Instruction`을 호스트 기능으로 변환하고 프로덕션 CAP 권한 게이트와 서명된 `Ego_Package` 검증 하에 제어된 작업을 실행합니다.

```bash
fayger version
fayger conformance
fayger demo live-ifay
```

전체 아키텍처 설계는 `blueprint/` 디렉터리의 문서를 참고하십시오.
