# Converts chatwoot db/schema.rb to plain PostgreSQL DDL without Rails.
# Usage: ruby tools/schema2sql.rb ../chatwoot/db/schema.rb > migrations/0001_schema.sql
# ponytail: covers only the DSL subset used by chatwoot schema.rb; extend when schema adds new options.
require 'json'
OUT = []
def q(s) = %("#{s}")
def lit(v)
  case v
  when Proc then v.call
  when String then "'#{v.gsub("'", "''")}'"
  when Array then "'{#{v.join(',')}}'"
  when nil then 'NULL'
  else v.to_s
  end
end

class Table
  TYPES = { string: 'varchar', text: 'text', integer: 'integer', bigint: 'bigint', boolean: 'boolean',
            datetime: 'timestamp', date: 'date', float: 'double precision', decimal: 'numeric',
            json: 'json', jsonb: 'jsonb', uuid: 'uuid', vector: 'vector', serial: 'serial', bigserial: 'bigserial' }.freeze
  attr_reader :cols, :idx, :checks
  def initialize(name) = (@name = name; @cols = []; @idx = []; @checks = [])
  TYPES.each_key do |t|
    define_method(t) { |n, **o| column(n, t, **o) }
  end
  def column(n, t, **o)
    sql = TYPES.fetch(t).dup
    sql = "varchar(#{o[:limit]})" if t == :string && o[:limit]
    sql = "vector(#{o[:limit]})" if t == :vector && o[:limit]
    sql = "numeric(#{o[:precision]},#{o[:scale]})" if t == :decimal && o[:precision]
    sql = "timestamp(#{o[:precision]})" if t == :datetime && o[:precision].is_a?(Integer)
    sql += '[]' if o[:array]
    if o.key?(:default)
      d = %i[json jsonb].include?(t) && !o[:default].is_a?(Proc) ? "'#{o[:default].to_json}'" : lit(o[:default])
      sql += " DEFAULT #{d}"
    end
    sql += ' NOT NULL' if o[:null] == false
    @cols << "#{q n} #{sql}"
  end
  def index(cols, name:, unique: false, where: nil, using: nil, opclass: nil, order: nil, **)
    cl = Array(cols).map do |c|
      next c if c.to_s.include?('(') || c.to_s.include?(' ')
      s = q(c)
      oc = opclass.is_a?(Hash) ? opclass[c.to_sym] : opclass
      s += " #{oc}" if oc
      s += " #{order[c.to_sym].to_s.upcase}" if order.is_a?(Hash) && order[c.to_sym]
      s
    end
    @idx << "CREATE #{'UNIQUE ' if unique}INDEX #{q name} ON %T#{" USING #{using}" if using} (#{cl.join(', ')})#{" WHERE #{where}" if where};"
  end
  def check_constraint(expr, name:) = @checks << "CONSTRAINT #{q name} CHECK (#{expr})"
  def timestamps(**o) = (datetime(:created_at, null: false, **o); datetime(:updated_at, null: false, **o))
end

module ActiveRecord
  class Schema
    def self.[](_) = self
    def self.define(version:, &blk)
      OUT << "-- generated from chatwoot schema.rb version #{version}"
      new.instance_eval(&blk)
      OUT << "CREATE TABLE IF NOT EXISTS schema_migrations (version varchar PRIMARY KEY);"
      OUT << "CREATE TABLE IF NOT EXISTS ar_internal_metadata (key varchar PRIMARY KEY, value varchar, created_at timestamp(6) NOT NULL, updated_at timestamp(6) NOT NULL);"
      OUT << "INSERT INTO schema_migrations VALUES ('#{version.to_s.delete('_')}') ON CONFLICT DO NOTHING;"
      puts OUT.join("\n")
    end
    def enable_extension(n) = OUT << "CREATE EXTENSION IF NOT EXISTS #{q n};"
    def create_table(name, id: :bigserial, force: nil, **)
      t = Table.new(name)
      idsql = { serial: 'serial', bigserial: 'bigserial', uuid: 'uuid DEFAULT gen_random_uuid()' }[id] if id
      t.cols << "\"id\" #{idsql} PRIMARY KEY" if id
      yield t
      OUT << "CREATE TABLE #{q name} (\n  #{(t.cols + t.checks).join(",\n  ")}\n);"
      t.idx.each { |i| OUT << i.sub('%T', q(name)) }
    end
    def add_foreign_key(from, to, column: nil, on_delete: nil, **)
      col = column || "#{to.to_s.sub(/ies\z/, 'y').sub(/(x|ss|sh|ch)es\z/, '\1').sub(/s\z/, '')}_id"
      act = { cascade: ' ON DELETE CASCADE', nullify: ' ON DELETE SET NULL' }[on_delete]
      OUT << "ALTER TABLE #{q from} ADD FOREIGN KEY (#{q col}) REFERENCES #{q to}(id)#{act};"
    end
    def create_trigger(name, **) = Trigger.new(name)
  end
end

class Trigger
  def initialize(n) = (@n = n; @name = n)
  def on(t) = (@t = t; self)
  def name(n) = (@name = n; self)
  def after(e) = (@when = "AFTER #{e.to_s.upcase}"; self)
  def before(e) = (@when = "BEFORE #{e.to_s.upcase}"; self)
  def for_each(_)
    body = yield
    OUT << <<~SQL
      CREATE OR REPLACE FUNCTION #{@name}() RETURNS TRIGGER AS $$ BEGIN #{body} RETURN NULL; END; $$ LANGUAGE plpgsql;
      CREATE TRIGGER #{@name} #{@when} ON #{q @t} FOR EACH ROW EXECUTE FUNCTION #{@name}();
    SQL
    OUT[-1] = OUT[-1].sub('RETURN NULL', 'RETURN NEW') if @when.start_with?('BEFORE')
    self
  end
end

load ARGV.fetch(0)
