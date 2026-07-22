# 项目说明

## 代码风格

- 所有新文件使用 TypeScript
- React 优先使用函数组件
- 使用 React Router 管理路由

## 版本同步

- 项目版本以 `VERSION` 为准。
- 更新版本时必须同步修改：
  - `VERSION`
  - `backend/Cargo.toml`
  - `backend/Cargo.lock` 中根包 `udx710`
  - `frontend/package.json`
- 不要只改单个文件；前后端展示版本和 CI 校验都依赖这几处保持一致。
- 版本变更后，提交前至少检查：
  - `cargo check --locked --manifest-path backend/Cargo.toml`
  - `pnpm lint`
  - `pnpm type-check`
