-- Project-local Neovim settings.
-- The global init.lua owns lazy.nvim; do not call lazy.setup() here.

vim.g.mapleader = " "
vim.g.maplocalleader = " "

-- Reuse the global Neo-tree installation, showing dotfiles in this project.
local lazy_ok, lazy = pcall(require, "lazy")
if lazy_ok then
  lazy.load({ plugins = { "neo-tree.nvim" } })
end

local neo_tree_ok = pcall(require, "neo-tree")
if neo_tree_ok then
  require("neo-tree").setup({
    close_if_last_window = false,
    filesystem = {
      filtered_items = {
        hide_dotfiles = false,
      },
      follow_current_file = {
        enabled = true,
      },
      use_libuv_file_watcher = true,
    },
    window = {
      position = "left",
      width = 30,
    },
  })
end

local map = vim.keymap.set

vim.api.nvim_create_autocmd("TermOpen", {
  callback = function(args)
    vim.bo[args.buf].modifiable = false
    vim.schedule(function()
      if vim.api.nvim_get_current_buf() == args.buf then
        vim.cmd("startinsert!")
      end
    end)
  end,
})

vim.api.nvim_create_autocmd("WinEnter", {
  callback = function()
    local buf = vim.api.nvim_get_current_buf()
    if vim.bo[buf].buftype == "terminal" then
      vim.schedule(function()
        if vim.api.nvim_get_current_buf() == buf then
          vim.cmd("startinsert!")
        end
      end)
    end
  end,
})

local function enter_terminal_mode(buf)
  vim.schedule(function()
    if vim.api.nvim_buf_is_valid(buf) and vim.api.nvim_get_current_buf() == buf then
      vim.cmd("startinsert!")
    end
  end)
end

local function focus_editor_window()
  for _, win in ipairs(vim.api.nvim_list_wins()) do
    local buf = vim.api.nvim_win_get_buf(win)
    if vim.bo[buf].filetype ~= "neo-tree" and vim.bo[buf].buftype ~= "terminal" then
      vim.api.nvim_set_current_win(win)
      return
    end
  end
end

local function toggle_terminal(key, split_command, terminal_command)
  local buf = vim.g[key]
  if type(buf) == "number" and vim.api.nvim_buf_is_valid(buf) then
    local wins = vim.fn.win_findbuf(buf)
    if #wins > 0 then
      vim.api.nvim_win_close(wins[1], true)
      return
    end

    focus_editor_window()
    vim.cmd(split_command)
    vim.api.nvim_win_set_buf(0, buf)
    enter_terminal_mode(buf)
    return
  end

  focus_editor_window()
  vim.cmd(split_command)
  if terminal_command then
    vim.cmd("terminal " .. terminal_command)
  else
    vim.cmd("terminal")
  end

  buf = vim.api.nvim_get_current_buf()
  vim.bo[buf].bufhidden = "hide"
  vim.g[key] = buf
  enter_terminal_mode(buf)
end

-- Give the Codex split its own title bar and a subtle tinted background so it
-- is easy to distinguish from both the editor and the regular shell.
vim.api.nvim_set_hl(0, "ProjectCodexBar", {
  fg = "#ffffff",
  bg = "#6846a5",
  bold = true,
})
vim.api.nvim_set_hl(0, "ProjectCodexTerminal", {
  fg = "#eee8fa",
  bg = "#30283b",
})
vim.api.nvim_set_hl(0, "ProjectShellBar", {
  fg = "#ffffff",
  bg = "#287c78",
  bold = true,
})
vim.api.nvim_set_hl(0, "ProjectShellTerminal", {
  fg = "#e5f5f3",
  bg = "#172726",
})
vim.api.nvim_set_hl(0, "ProjectShellTerminalNC", {
  fg = "#bfd8d5",
  bg = "#172726",
})
local function style_codex_window()
  if vim.bo.buftype ~= "terminal" or vim.api.nvim_get_current_buf() ~= vim.g.project_codex_buf then
    return
  end
  vim.wo.winbar = "%#ProjectCodexBar#  CODEX  ·  LAST SESSION  "
  vim.wo.winhighlight = "Normal:ProjectCodexTerminal,NormalNC:ProjectCodexTerminal"
end

local function style_shell_window()
  if vim.bo.buftype ~= "terminal" or vim.api.nvim_get_current_buf() ~= vim.g.project_shell_buf then
    return
  end
  vim.wo.winbar = "%#ProjectShellBar#  SHELL  ·  PROJECT TERMINAL  "
  vim.wo.winhighlight = "Normal:ProjectShellTerminal,NormalNC:ProjectShellTerminalNC"
end

local function toggle_from_terminal(callback)
  return function()
    vim.g.project_terminal_toggle_in_progress = true
    local normal_mode = vim.api.nvim_replace_termcodes("<C-\\><C-n>", true, false, true)
    vim.api.nvim_feedkeys(normal_mode, "n", false)
    vim.schedule(function()
      callback()
      vim.g.project_terminal_toggle_in_progress = false
    end)
  end
end

local function toggle_shell()
  toggle_terminal("project_shell_buf", "botright 12split")
  style_shell_window()
end
map("n", "<C-t>", toggle_shell, { desc = "Toggle project shell at bottom" })
map("t", "<C-t>", toggle_from_terminal(toggle_shell), { desc = "Toggle project shell at bottom" })

local function toggle_codex()
  toggle_terminal("project_codex_buf", "botright 60vsplit", "codex resume --last")
  style_codex_window()
end
map("n", "<C-g>", toggle_codex, { desc = "Toggle Codex session on the right" })
map("t", "<C-g>", toggle_from_terminal(toggle_codex), { desc = "Toggle Codex session on the right" })

local function open_neogit()
  vim.cmd("Neogit kind=split")
end
map("n", "<C-b>", open_neogit, { desc = "Open Neogit Git status panel" })
map("t", "<C-b>", toggle_from_terminal(open_neogit), { desc = "Open Neogit Git status panel" })

map("n", "<leader>e", "<cmd>Neotree toggle left<CR>", { desc = "Toggle file explorer" })
map("n", "<leader>o", "<cmd>Neotree filesystem reveal left<CR>", { desc = "Reveal current file" })
map("n", "<Tab>", "<cmd>BufferLineCycleNext<CR>", { desc = "Next tab" })
map("n", "<S-Tab>", "<cmd>BufferLineCyclePrev<CR>", { desc = "Previous tab" })
map("n", "<leader>x", "<cmd>bdelete<CR>", { desc = "Close current file" })
map("n", "<leader>w", "<cmd>write<CR>", { desc = "Save file" })
map("n", "<leader>q", "<cmd>qa<CR>", { desc = "Quit Neovim" })
