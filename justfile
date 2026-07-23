# Windows 호스트에서 현재 StarPrison 컨테이너의 Codex 세션을 관리합니다.

resume:
    docker exec -it -w /workspaces/StarPrison starprison-workspace-workspace-1 codex resume --all

last:
    docker exec -it -w /workspaces/StarPrison starprison-workspace-workspace-1 codex resume --last

shell:
    docker exec -it -w /workspaces/StarPrison starprison-workspace-workspace-1 bash
