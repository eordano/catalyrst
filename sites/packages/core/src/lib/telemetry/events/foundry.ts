export type FoundryEvents = {
  fd_bench_report_viewed: {
    slug: string;
    verdict: "fail" | "none" | "pass";
  };
  fd_console_module_viewed: {
    module: "bench" | "costs" | "trajectories";
  };
  fd_copilot_opened: {
    online: boolean;
  };
  fd_copilot_viewed: {
    online: boolean;
    sessions_total: number;
    tokens_total: number;
  };
  fd_costs_viewed: {
    messages: number;
    tokens_total: number;
  };
  fd_embed_started: {
    reachable: boolean;
    slug: string;
  };
  fd_exchange_viewed: {
    pledges_total: number;
    requests_open: number;
  };
  fd_game_link_opened: {
    slug: string;
    target: "editor" | "play" | "world";
  };
  fd_game_viewed: {
    slug: string;
  };
  fd_gdd_list_viewed: {
    docs: number;
  };
  fd_gdd_skill_link_clicked: {
    skill: string;
  };
  fd_gdd_viewed: {
    doc_id: string;
    hypotheses: number;
    kind: string;
    open_sections: number;
  };
  fd_home_viewed: {
    bench_runs: number;
    copilot_online: boolean;
    gdd_total: number;
    scenes_total: number;
  };
  fd_idea_expanded: {
    idea_id: string;
  };
  fd_play_viewed: {
    scenes_total: number;
  };
  fd_pledge_retracted: {
    pledges_after: number;
    request_id: string;
  };
  fd_pledge_submitted: {
    pledges_after: number;
    request_id: string;
  };
  fd_replay_opened: {
    events: number;
    trajectory_id: string;
  };
  fd_replay_scrubbed: {
    seq: number;
    trajectory_id: string;
  };
  fd_replay_stepped: {
    direction: "back" | "forward";
    trajectory_id: string;
  };
  fd_request_submitted: {
    request_id: string;
    title_len: number;
  };
  fd_role_chosen: {
    destination: "console" | "editor" | "play";
    role: "admin" | "create" | "start";
  };
  fd_select_viewed: {
    roles_shown: number;
  };
  fd_tour_completed: {
    steps: number;
  };
  fd_tour_dismissed: {
    step: number;
    steps: number;
  };
  fd_tour_started: {
    page: string;
    source: "auto" | "button";
    steps: number;
  };
  fd_tour_step_viewed: {
    page: string;
    step: number;
    step_id: string;
    steps: number;
  };
  fd_trajectory_inspected: {
    provenance: "bot" | "visitor";
    trajectory_id: string;
  };
};
