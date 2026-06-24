#!/usr/bin/env bash
set -euo pipefail

# 自动打 tag 并推送触发发布
# 用法:
#   ./scripts/release.sh              # 自动递增 patch 版本
#   ./scripts/release.sh minor        # 递增 minor 版本
#   ./scripts/release.sh major        # 递增 major 版本
#   ./scripts/release.sh agents-v0.3.0 # 手动指定版本号

BUMP="${1:-patch}"

# 获取最新的 agents-v 开头的 tag
latest_tag=$(git tag --sort=-v:refname | grep '^agents-v' | head -1)

if [ -z "$latest_tag" ]; then
  echo "未找到任何 Agent 版本 tag，从 agents-v0.1.0 开始"
  latest_tag="agents-v0.0.0"
fi

echo "当前最新 tag: $latest_tag"

# 去掉 agents-v 前缀
version="${latest_tag#agents-v}"
IFS='.' read -r major minor patch <<< "$version"

# 计算新版本号
case "$BUMP" in
  major)
    new_tag="agents-v$((major + 1)).0.0"
    ;;
  minor)
    new_tag="agents-v${major}.$((minor + 1)).0"
    ;;
  patch)
    new_tag="agents-v${major}.${minor}.$((patch + 1))"
    ;;
  agents-v*)
    new_tag="$BUMP"
    ;;
  *)
    echo "错误: 未知的版本递增方式 '$BUMP'，请使用 patch/minor/major 或 agents-vX.Y.Z"
    exit 1
    ;;
esac

echo "新版本: $new_tag"

# 确认
read -r -p "确认创建并推送 tag ${new_tag}? [y/N] " confirm
if [ "$confirm" != "y" ] && [ "$confirm" != "Y" ]; then
  echo "已取消"
  exit 0
fi

# 创建并推送 tag
git tag "$new_tag"
git push origin "$new_tag"

echo "完成! tag $new_tag 已推送，CI 将自动触发发布流程。"
