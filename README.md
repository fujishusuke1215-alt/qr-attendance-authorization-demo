# Shiftline — QR Attendance & HR Authorization Demo

![CI](https://github.com/fujishusuke1215-alt/qr-attendance-authorization-demo/actions/workflows/ci.yml/badge.svg)

独立したポートフォリオ用デモです。QRによる出退勤、外部HRスナップショットの同期、部門単位の自動・手動権限を、Rust/Axum/PostgreSQLとReact/Viteで実装しています。社員名、所属、職務コードはすべて架空です。実在企業での稼働実績や本番導入を示すものではありません。

参考にしたのは[backend-design-case-studies の現在の `main` にある対象ディレクトリ](https://github.com/fujishusuke1215-alt/backend-design-case-studies/tree/main/qr-attendance-authorization)の機能要件と設計課題です。コードのコピー、改名、機械的な書き換えは行わず、新しいモデルとモジュール、画面、テストを設計しました。実装とテストの照合は[機能対応表](docs/FEATURE_MATRIX.md)にまとめています。

## 起動

Docker Desktop が起動している環境で、リポジトリのルートから実行します。

```sh
docker compose up --build
```

ブラウザで <http://localhost:5173> を開きます。ローカル開発用ログインには、次の架空ユーザーを選べます。

| ユーザー | 用途 |
|---|---|
| Aoi Admin (`admin@example.test`) | QR更新、権限管理、HR同期 |
| Ren Manager (`manager@example.test`) | Engineering 部門の打刻閲覧 |
| Mio Staff (`staff@example.test`) | 自分の出退勤 |

PostgreSQLの準備後、バックエンドが起動時にマイグレーションと架空の初期データを適用します。管理画面で「HR同期」を実行すると、同梱の完全なHRスナップショットを使って部門・社員・自動権限が同期されます。ローカルの設定値はComposeにあるダミー値です。独自のローカル値を使う場合は `.env.example` を `.env` にコピーして変更してください。

## 実装範囲

- 出勤用と退勤用のQRを個別のHMAC-SHA256トークンとして発行します。QRは日付で失効せず、管理者がリビジョンを進めると旧QRが使えなくなります。掲示中のコードを差し替える前に確認画面を表示します。
- 打刻は認証済みの社員プロフィール、現行QR、Tokyoの勤務日をサーバーで検証します。同一利用者・同一勤務日・同一アクションは最大1件で、`Idempotency-Key` を使った再送は元のイベントを返します。
- HR同期は部門、社員プロフィール・所属、自動権限の段階を記録します。部門IDは `identity_code` から安定させ、表示名や親部門の変更でIDを変えません。コード移行は入力に明示された別名だけを適用します。
- 手動権限とHR自動権限は別々の行です。HR同期は自動行だけを追加・取消しし、手動付与を降格・削除しません。手動変更とQR更新は追記専用監査ログに記録します。
- React画面を認証、出退勤、QR管理、権限管理、部門ツリー、HR同期状況に分けています。管理画面の保護はバックエンドでも検証します。
- Microsoft Entra ID/MSALとローカル開発用認証をアダプターで切り替えます。ログイン後の戻り先は許可したアプリ内パスに限定します。

## 技術構成

| 領域 | 採用 |
|---|---|
| Backend | Rust 1.100.0、Axum 0.8.9、SQLx 0.9、Tokio |
| Database | PostgreSQL 18 |
| Frontend | React 19.3、Vite 8.3、TypeScript 6.0、React Router 7 |
| Authentication | Microsoft Entra ID / MSAL Browser 5.29、ローカル開発アダプター |
| API | REST、[OpenAPI 3.1](docs/openapi.yaml)、実行時仕様 `/api/openapi.yaml` |
| Local | Docker Compose |
| CI/CD | GitHub Actions、タグリリースでGHCRへAPI/UIイメージを公開 |

Rust、Axum、SQLxの互換性はSQLx 0.9のTokio runtime/TLS機能とAxum 0.8系を確認し、Cargo.lockに解決結果を固定しています。フロントエンドもNode 24上でVite 8、React 19、MSAL Browserの組み合わせを固定しています。

## APIと画面

| API | 用途 |
|---|---|
| `GET /api/me` | 現在の社員プロフィールとバックエンド権限 |
| `GET /api/attendance/today` | 自分の出勤・退勤状況 |
| `POST /api/attendance/scan` | 署名QRの検証と打刻。`Idempotency-Key` が必要 |
| `GET /api/attendance/scope?date=YYYY-MM-DD` | 管理者または付与済み部門の閲覧範囲 |
| `GET /api/admin/qr` / `POST /api/admin/qr/{kind}/rotate` | 現行QRと履歴、リビジョン更新 |
| `GET /api/admin/departments/tree` | HR由来の部門階層と人数 |
| `GET /api/admin/permissions` / `POST|DELETE /api/admin/permissions/...` | 権限の付与元表示、手動付与と取消し |
| `GET|POST /api/admin/hr-sync` / `GET /api/admin/hr-sync/runs/{id}` | 手動同期と段階ごとの結果 |
| `GET /api/admin/audit` | 監査履歴 |

詳細なパス、入力、応答は[OpenAPI定義](docs/openapi.yaml)を参照してください。主な画面は `/attendance`、`/admin/qr`、`/admin/permissions`、`/admin/hr-sync` です。管理者・マネージャーにはバックエンドで許可された範囲の打刻一覧を出退勤画面に表示します。

## HRスナップショット形式

実サービスへの接続は行いません。`backend/src/sync/` の `HrSource` アダプターが `backend/fixtures/hr_snapshot.json` を読み込みます。実HR連携に置き換える場合も、取得アダプターが同じ正規化済みモデルを返す設計です。

主要フィールドは次のとおりです。

- `snapshot_id`, `complete`: 全ページ取得済みを示すスナップショット識別子と完了フラグ
- `departments[]`: 不変の `identity_code`、主コード `code`、明示的な `aliases[]`、名称、親identity、active
- `employees[]`: `employee_code`、氏名、メール、`job_code`、`job_code_field_present`、所属identityの配列、active

職務コード項目が省略されたデータは、職務コードなしとは解釈しません。検証段階で同期全体を失敗にし、自動権限の取消しへ進みません。同じ表示名でもidentityが違う部門は別の部門です。移行コードは `aliases` に含めた場合だけ、指定されたidentityに結び付きます。

## 認証設定

ローカルComposeは `APP_ENV=development`、`AUTH_MODE=dev`、`VITE_AUTH_MODE=dev` で動きます。ローカルアダプターは `X-Dev-User` の架空ユーザーIDを使い、バックエンドの `APP_ENV=production` では起動できません。

Entra IDへ切り替える場合は、テナント内にSPAアプリとAPIアプリを登録し、APIに `access_as_user` スコープを公開します。SPAのリダイレクトURIに利用するアプリURLを登録し、次の値を設定します。

```dotenv
APP_ENV=production
AUTH_MODE=entra
ENTRA_TENANT_ID=<tenant-guid>
ENTRA_API_AUDIENCE=api://<api-client-guid>
VITE_AUTH_MODE=entra
VITE_ENTRA_TENANT_ID=<tenant-guid>
VITE_ENTRA_CLIENT_ID=<spa-client-guid>
VITE_ENTRA_API_SCOPE=api://<api-client-guid>/access_as_user
```

バックエンドはRS256署名、MicrosoftのJWKS、issuer、audience、時刻クレームを検証し、Entra `oid` に対応する有効な社員レコードを探します。実ユーザーとサンプルレコードの紐付けは、承認済みの社員プロビジョニング手順で `users.entra_oid` に設定します。ソースや `.env.example` に実資格情報は置きません。

QR署名鍵は `QR_HMAC_KEY_BASE64` から読み込み、32バイト以上を要求します。ローカル用の初期値はダミーです。環境に合わせた鍵は秘密管理基盤から渡してください。鍵を変更すると、その鍵で生成した全QRが無効になるため、QRリビジョン更新と掲示差し替えを行える運用が必要です。

## テスト、ビルド、CI/CD

バックエンドの単体テスト:

```sh
cd backend
cargo fmt --all --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --all-targets
```

PostgreSQL受け入れテストを含める場合は、`TEST_DATABASE_URL` をマイグレーション可能なPostgreSQLデータベースへ設定します。GitHub ActionsではPostgreSQL 18サービスを起動して全テストを実行します。

フロントエンド:

```sh
cd frontend
pnpm install --frozen-lockfile
pnpm typecheck
pnpm test
pnpm build
```

Pull Requestと `main` 更新のCIはRust format、Clippy、unit/DB integration tests、フロントエンド型チェック・テスト・build、両Dockerイメージのbuildを行います。`v1.0.0` 形式のタグをpushすると、GitHub Container Registryへbackend/frontendイメージを公開します。公開はGitHub Actionsのワークフローが実行されてから確認してください。

## DB構成と設計資料

SQLマイグレーションは `backend/migrations/` にあります。主なテーブルは `users`、`departments`、`department_codes`、`employee_departments`、`user_roles`、`manager_scopes`、`qr_channels`、`qr_revision_history`、`attendance_events`、`audit_events`、`hr_sync_runs`、`hr_sync_stages` です。

- [設計判断とデータ整合性](docs/DESIGN.md)
- [機能対応表と検証結果](docs/FEATURE_MATRIX.md)
- [OpenAPI 3.1](docs/openapi.yaml)

## 明示している境界

- HRアダプターは再現可能な架空JSONのみです。実HRベンダーのネットワーク接続や資格情報の管理は含みません。
- 部門・職務コードのポリシーはデモ用で、実会社の職務体系や雇用ルールを表しません。
- 1日につき出勤1件・退勤1件とし、夜勤の勤務日判定、休憩、シフト、給与連携、位置情報による不正防止は対象外です。
- Entraの `oid` と社員レコードの紐付けは明示的なプロビジョニングが必要です。メールアドレスだけで自動的にアカウントを結び付けません。
- このリポジトリはポートフォリオ用の独立デモであり、実運用での可用性・監査認証・本番導入を主張しません。
