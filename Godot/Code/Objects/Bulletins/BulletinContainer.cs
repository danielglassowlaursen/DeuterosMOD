using Deuteros.Code.Objects.ModuleTextFrame;
using System;
using System.Collections.Generic;
using System.Linq;
using System.Text;
using System.Threading.Tasks;

namespace Deuteros.Code.Objects.Bulletins
{
	public class BulletinContainer
	{
		private List<Bulletin> Bulletins { get; set; }

		public BulletinContainer()
		{
			Bulletins = new List<Bulletin>();
		}

		public void Add(Bulletin bulletin)
		{
			Bulletins.Add(bulletin);
		}

		public Bulletin this[Enums.BulletinTypes index]
		{
			get => Bulletins.Single(T => T.BulletinTypeName == index);
		}
	}
}